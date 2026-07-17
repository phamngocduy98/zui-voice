use crate::types::{AppError, AppResult, DownloadProgress, SetupStatus};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

pub const MODEL_FILENAME: &str = "parakeet-ctc-0.6b-Vietnamese-q8_0.gguf";

#[cfg(windows)]
pub const SERVER_FILENAME: &str = "parakeet-server.exe";
#[cfg(not(windows))]
pub const SERVER_FILENAME: &str = "parakeet-server";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub release: String,
    pub engine_version: String,
    pub license_notice_url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAsset {
    pub id: String,
    pub kind: String,
    pub platform: String,
    pub arch: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct BackendPaths {
    pub server: PathBuf,
    pub model: PathBuf,
}

pub struct AssetManager {
    app: AppHandle,
    install_dir: PathBuf,
    cancelled: AtomicBool,
}

impl AssetManager {
    pub fn new(app: &AppHandle) -> AppResult<Self> {
        let install_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::fatal("app_data_dir", e.to_string()))?
            .join("assets");
        fs::create_dir_all(&install_dir)
            .map_err(|e| AppError::fatal("asset_dir", e.to_string()))?;
        Ok(Self {
            app: app.clone(),
            install_dir,
            cancelled: AtomicBool::new(false),
        })
    }

    pub fn manifest_url(&self) -> Option<String> {
        std::env::var("ZUI_RELEASE_MANIFEST_URL")
            .ok()
            .or_else(|| option_env!("ZUI_RELEASE_MANIFEST_URL").map(str::to_owned))
            .filter(|url| !url.trim().is_empty())
    }

    pub fn resolve_paths(&self) -> Option<BackendPaths> {
        let installed = BackendPaths {
            server: self.install_dir.join(SERVER_FILENAME),
            model: self.install_dir.join(MODEL_FILENAME),
        };
        if installed.server.is_file() && installed.model.is_file() {
            return Some(installed);
        }

        for root in development_roots() {
            let candidate = BackendPaths {
                server: root.join("bin").join(SERVER_FILENAME),
                model: root.join("bin").join(MODEL_FILENAME),
            };
            if candidate.server.is_file() && candidate.model.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    pub fn status(&self) -> SetupStatus {
        let paths = self.resolve_paths();
        let server_path = paths.as_ref().map(|p| p.server.clone());
        let model_path = paths.as_ref().map(|p| p.model.clone());
        SetupStatus {
            complete: paths.is_some(),
            server_found: server_path.is_some(),
            model_found: model_path.is_some(),
            server_path,
            model_path,
            manifest_configured: self.manifest_url().is_some(),
        }
    }

    pub async fn download(&self) -> AppResult<SetupStatus> {
        self.cancelled.store(false, Ordering::Release);
        let url = self.manifest_url().ok_or_else(|| {
            AppError::new(
                "manifest_missing",
                "Set ZUI_RELEASE_MANIFEST_URL to a published release manifest.",
            )
        })?;
        let client = reqwest::Client::new();
        let manifest: ReleaseManifest = client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::new("manifest_download", e.to_string()))?
            .error_for_status()
            .map_err(|e| AppError::new("manifest_download", e.to_string()))?
            .json()
            .await
            .map_err(|e| AppError::new("manifest_invalid", e.to_string()))?;
        validate_manifest(&manifest)?;

        let platform = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let selected: Vec<_> = manifest
            .assets
            .iter()
            .filter(|asset| {
                (asset.platform == "all" || asset.platform == platform)
                    && (asset.arch == "all" || asset.arch == arch)
                    && (asset.kind == "model" || asset.kind == "runtime")
            })
            .collect();
        if !selected.iter().any(|asset| asset.kind == "model")
            || !selected.iter().any(|asset| asset.kind == "runtime")
        {
            return Err(AppError::new(
                "asset_unsupported",
                format!("No complete asset set is published for {platform}/{arch}."),
            ));
        }

        for asset in selected {
            self.ensure_not_cancelled()?;
            self.download_asset(&client, asset).await?;
        }
        Ok(self.status())
    }

    async fn download_asset(
        &self,
        client: &reqwest::Client,
        asset: &ReleaseAsset,
    ) -> AppResult<()> {
        let destination = self.install_dir.join(&asset.filename);
        self.ensure_not_cancelled()?;
        if destination.is_file()
            && sha256_file(destination.clone()).await? == asset.sha256.to_lowercase()
        {
            return Ok(());
        }

        let partial = destination.with_extension(format!(
            "{}.part",
            destination
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("asset")
        ));
        let received = tokio::fs::metadata(&partial)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut request = client.get(&asset.url);
        if received > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={received}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|e| AppError::new("asset_download", e.to_string()))?;
        let append = response.status() == reqwest::StatusCode::PARTIAL_CONTENT && received > 0;
        let response = response
            .error_for_status()
            .map_err(|e| AppError::new("asset_download", e.to_string()))?;
        let mut downloaded = if append { received } else { 0 };
        let mut file = if append {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&partial)
                .await
        } else {
            tokio::fs::File::create(&partial).await
        }
        .map_err(|e| AppError::new("asset_write", e.to_string()))?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            self.ensure_not_cancelled()?;
            let chunk = chunk.map_err(|e| AppError::new("asset_download", e.to_string()))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| AppError::new("asset_write", e.to_string()))?;
            downloaded += chunk.len() as u64;
            let _ = self.app.emit(
                "voice://download-progress",
                DownloadProgress {
                    asset: asset.id.clone(),
                    received: downloaded,
                    total: Some(asset.size),
                    percent: (asset.size > 0)
                        .then_some((downloaded as f64 / asset.size as f64 * 100.0).min(100.0)),
                },
            );
        }
        file.flush()
            .await
            .map_err(|e| AppError::new("asset_write", e.to_string()))?;
        drop(file);

        let actual = sha256_file(partial.clone()).await?;
        if actual != asset.sha256.to_lowercase() {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(AppError::new(
                "checksum_mismatch",
                format!("Checksum verification failed for {}.", asset.filename),
            ));
        }
        if destination.exists() {
            let _ = tokio::fs::remove_file(&destination).await;
        }
        tokio::fs::rename(&partial, &destination)
            .await
            .map_err(|e| AppError::new("asset_install", e.to_string()))?;
        make_executable_if_runtime(&destination, &asset.kind)?;
        Ok(())
    }

    pub fn cancel_download(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn ensure_not_cancelled(&self) -> AppResult<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(AppError::new(
                "download_cancelled",
                "Asset download was cancelled and can be resumed later.",
            ))
        } else {
            Ok(())
        }
    }
}

fn development_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(debug_assertions)]
    if let Some(workspace) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        roots.push(workspace.to_path_buf());
    }
    if let Ok(current) = std::env::current_dir() {
        roots.push(current.clone());
        if let Some(parent) = current.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    roots
}

fn validate_manifest(manifest: &ReleaseManifest) -> AppResult<()> {
    if manifest.schema_version != 1 {
        return Err(AppError::new(
            "manifest_version",
            format!(
                "Unsupported release manifest version {}.",
                manifest.schema_version
            ),
        ));
    }
    for asset in &manifest.assets {
        if asset.sha256.len() != 64 || hex::decode(&asset.sha256).is_err() {
            return Err(AppError::new(
                "manifest_checksum",
                format!("{} has an invalid SHA-256.", asset.id),
            ));
        }
        let parsed = url::Url::parse(&asset.url)
            .map_err(|e| AppError::new("manifest_url", e.to_string()))?;
        if parsed.scheme() != "https" {
            return Err(AppError::new(
                "manifest_url",
                "Release assets must use HTTPS.",
            ));
        }
        if Path::new(&asset.filename)
            .file_name()
            .and_then(|v| v.to_str())
            != Some(asset.filename.as_str())
        {
            return Err(AppError::new(
                "manifest_path",
                "Asset filenames cannot contain paths.",
            ));
        }
    }
    Ok(())
}

async fn sha256_file(path: PathBuf) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let mut file =
            fs::File::open(path).map_err(|e| AppError::new("asset_read", e.to_string()))?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|e| AppError::new("asset_read", e.to_string()))?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(hex::encode(hash.finalize()))
    })
    .await
    .map_err(|e| AppError::new("asset_hash", e.to_string()))?
}

fn make_executable_if_runtime(path: &Path, kind: &str) -> AppResult<()> {
    #[cfg(not(unix))]
    let _ = path;
    if kind != "runtime" {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|e| AppError::new("asset_permissions", e.to_string()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|e| AppError::new("asset_permissions", e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https_assets() {
        let manifest = ReleaseManifest {
            schema_version: 1,
            release: "1".into(),
            engine_version: "1".into(),
            license_notice_url: "https://example.com".into(),
            assets: vec![ReleaseAsset {
                id: "bad".into(),
                kind: "model".into(),
                platform: "all".into(),
                arch: "all".into(),
                filename: "model.gguf".into(),
                url: "http://example.com/model".into(),
                size: 1,
                sha256: "0".repeat(64),
            }],
        };
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_invalid_checksum_metadata() {
        let manifest = ReleaseManifest {
            schema_version: 1,
            release: "1".into(),
            engine_version: "1".into(),
            license_notice_url: "https://example.com".into(),
            assets: vec![ReleaseAsset {
                id: "bad-hash".into(),
                kind: "model".into(),
                platform: "all".into(),
                arch: "all".into(),
                filename: "model.gguf".into(),
                url: "https://example.com/model".into(),
                size: 1,
                sha256: "not-a-sha256".into(),
            }],
        };
        assert_eq!(
            validate_manifest(&manifest).expect_err("invalid hash").code,
            "manifest_checksum"
        );
    }
}
