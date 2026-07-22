use crate::{
    cancellation::CancellationSignal,
    models,
    types::{AppError, AppResult, DownloadPhase, DownloadProgress, SetupStatus},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

const DEFAULT_RELEASE_MANIFEST_URL: &str = concat!(
    "https://github.com/phamngocduy98/zui-voice/releases/download/v",
    env!("CARGO_PKG_VERSION"),
    "/release-manifest.json"
);
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(60);

#[cfg(windows)]
pub const SERVER_FILENAME: &str = "parakeet-server.exe";
#[cfg(not(windows))]
pub const SERVER_FILENAME: &str = "parakeet-server";

const ENGINE_VERSION_MARKER: &str = ".parakeet-engine-version";
const REQUIRED_ENGINE_VERSION: &str = "0.4.0-zui.2";

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
    #[serde(default)]
    pub backend_id: Option<String>,
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
    cancellation: CancellationSignal,
    download_lock: tokio::sync::Mutex<()>,
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
        recover_interrupted_install(&install_dir.join(SERVER_FILENAME))?;
        for backend in models::BACKENDS {
            recover_interrupted_install(&install_dir.join(backend.model_filename))?;
        }
        ensure_development_engine_marker()?;
        Ok(Self {
            app: app.clone(),
            install_dir,
            cancellation: CancellationSignal::default(),
            download_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn manifest_url(&self) -> Option<String> {
        std::env::var("ZUI_RELEASE_MANIFEST_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
            .or_else(|| {
                option_env!("ZUI_RELEASE_MANIFEST_URL")
                    .filter(|url| !url.trim().is_empty())
                    .map(str::to_owned)
            })
            .or_else(|| Some(DEFAULT_RELEASE_MANIFEST_URL.to_owned()))
    }

    pub fn resolve_paths(&self, backend_id: &str) -> Option<BackendPaths> {
        self.path_candidates(backend_id)
            .into_iter()
            .find(|paths| self.paths_match_required_engine(paths))
    }

    fn paths_match_required_engine(&self, paths: &BackendPaths) -> bool {
        paths.server.is_file()
            && paths.model.is_file()
            && engine_version_matches(&paths.server, REQUIRED_ENGINE_VERSION)
    }

    pub fn server_path(&self) -> Option<PathBuf> {
        let installed = self.install_dir.join(SERVER_FILENAME);
        if installed.is_file() {
            return Some(installed);
        }
        development_roots()
            .into_iter()
            .map(|root| root.join("bin").join(SERVER_FILENAME))
            .find(|path| path.is_file())
    }

    fn path_candidates(&self, backend_id: &str) -> Vec<BackendPaths> {
        let Some(backend) = models::backend(backend_id) else {
            return Vec::new();
        };
        let mut candidates = vec![BackendPaths {
            server: self.install_dir.join(SERVER_FILENAME),
            model: self.install_dir.join(backend.model_filename),
        }];
        candidates.extend(development_roots().into_iter().map(|root| BackendPaths {
            server: root.join("bin").join(SERVER_FILENAME),
            model: root.join("bin").join(backend.model_filename),
        }));
        candidates
    }

    pub fn status(&self, backend_id: &str) -> SetupStatus {
        let candidates = self.path_candidates(backend_id);
        let complete_paths = candidates
            .iter()
            .find(|paths| self.paths_match_required_engine(paths));
        let server_path = candidates
            .iter()
            .find(|paths| paths.server.is_file())
            .map(|paths| paths.server.clone());
        let model_path = candidates
            .iter()
            .find(|paths| paths.model.is_file())
            .map(|paths| paths.model.clone());
        SetupStatus {
            backend_id: backend_id.into(),
            complete: complete_paths.is_some(),
            server_found: server_path.is_some(),
            model_found: model_path.is_some(),
            server_path,
            model_path,
            manifest_configured: self.manifest_url().is_some(),
        }
    }

    pub async fn download(&self, backend_id: &str) -> AppResult<SetupStatus> {
        if models::backend(backend_id).is_none() {
            return Err(AppError::new(
                "unsupported_backend",
                "The selected transcription backend is not supported.",
            ));
        }
        let _download = self.download_lock.try_lock().map_err(|_| {
            AppError::new(
                "download_in_progress",
                "An asset download is already in progress.",
            )
        })?;
        let generation = self.cancellation.generation();
        let url = self.manifest_url().ok_or_else(|| {
            AppError::new(
                "manifest_missing",
                "Set ZUI_RELEASE_MANIFEST_URL to a published release manifest.",
            )
        })?;
        let url = validate_https_url(&url, "Release manifest")?;
        self.emit_progress(DownloadPhase::FetchingManifest, "manifest", 0, None);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.url().scheme() != "https" {
                    attempt.error("Refusing to follow a non-HTTPS redirect")
                } else if attempt.previous().len() >= 10 {
                    attempt.error("Too many redirects")
                } else {
                    attempt.follow()
                }
            }))
            .user_agent(concat!("zui-voice/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::new("asset_client", e.to_string()))?;
        let response = self
            .cancellation
            .run(generation, client.get(url).send())
            .await
            .ok_or_else(download_cancelled)?
            .map_err(|e| AppError::new("manifest_download", e.to_string()))?
            .error_for_status()
            .map_err(|e| AppError::new("manifest_download", e.to_string()))?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_MANIFEST_BYTES as u64)
        {
            return Err(AppError::new(
                "manifest_too_large",
                "The release manifest is larger than 1 MiB.",
            ));
        }
        let mut manifest_bytes = Vec::new();
        let mut manifest_stream = response.bytes_stream();
        loop {
            let next = self
                .cancellation
                .run(
                    generation,
                    tokio::time::timeout(DOWNLOAD_STALL_TIMEOUT, manifest_stream.next()),
                )
                .await
                .ok_or_else(download_cancelled)?
                .map_err(|_| AppError::new("manifest_timeout", "The manifest download stalled."))?;
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|e| AppError::new("manifest_download", e.to_string()))?;
            if manifest_bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
                return Err(AppError::new(
                    "manifest_too_large",
                    "The release manifest is larger than 1 MiB.",
                ));
            }
            manifest_bytes.extend_from_slice(&chunk);
        }
        let manifest: ReleaseManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| AppError::new("manifest_invalid", e.to_string()))?;
        validate_manifest(&manifest)?;

        let platform = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let selected = select_asset_set(&manifest, platform, arch, backend_id)?;

        let engine_version = manifest.engine_version.clone();
        for asset in selected {
            self.ensure_not_cancelled(generation)?;
            self.download_asset(&client, asset, generation).await?;
        }
        self.write_engine_version(&engine_version).await?;
        Ok(self.status(backend_id))
    }

    async fn download_asset(
        &self,
        client: &reqwest::Client,
        asset: &ReleaseAsset,
        generation: u64,
    ) -> AppResult<()> {
        let destination = self.install_dir.join(&asset.filename);
        self.ensure_not_cancelled(generation)?;
        self.emit_progress(DownloadPhase::Verifying, &asset.id, 0, Some(asset.size));
        if destination.is_file()
            && self.sha256(destination.clone(), generation).await? == asset.sha256.to_lowercase()
        {
            make_executable_if_runtime(&destination, &asset.kind)?;
            return Ok(());
        }

        let partial = destination.with_extension(format!(
            "{}.part",
            destination
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("asset")
        ));
        let mut received = tokio::fs::metadata(&partial)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if received > asset.size {
            tokio::fs::remove_file(&partial)
                .await
                .map_err(|e| AppError::new("asset_write", e.to_string()))?;
            received = 0;
        }
        if received == asset.size && received > 0 {
            self.emit_progress(
                DownloadPhase::Verifying,
                &asset.id,
                received,
                Some(asset.size),
            );
            if self.sha256(partial.clone(), generation).await? == asset.sha256.to_lowercase() {
                self.emit_progress(
                    DownloadPhase::Installing,
                    &asset.id,
                    received,
                    Some(asset.size),
                );
                install_verified_asset(&partial, &destination).await?;
                make_executable_if_runtime(&destination, &asset.kind)?;
                return Ok(());
            }
            tokio::fs::remove_file(&partial)
                .await
                .map_err(|e| AppError::new("asset_write", e.to_string()))?;
            received = 0;
        }
        let mut request = client.get(&asset.url);
        if received > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={received}-"));
        }
        self.emit_progress(
            DownloadPhase::Downloading,
            &asset.id,
            received,
            Some(asset.size),
        );
        let response = self
            .cancellation
            .run(generation, request.send())
            .await
            .ok_or_else(download_cancelled)?
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
        loop {
            let next = self
                .cancellation
                .run(
                    generation,
                    tokio::time::timeout(DOWNLOAD_STALL_TIMEOUT, stream.next()),
                )
                .await
                .ok_or_else(download_cancelled)?
                .map_err(|_| AppError::new("asset_timeout", "The asset download stalled."))?;
            let Some(chunk) = next else { break };
            self.ensure_not_cancelled(generation)?;
            let chunk = chunk.map_err(|e| AppError::new("asset_download", e.to_string()))?;
            if downloaded.saturating_add(chunk.len() as u64) > asset.size {
                drop(file);
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(AppError::new(
                    "asset_size",
                    format!("{} exceeded its declared size.", asset.filename),
                ));
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| AppError::new("asset_write", e.to_string()))?;
            downloaded += chunk.len() as u64;
            self.emit_progress(
                DownloadPhase::Downloading,
                &asset.id,
                downloaded,
                Some(asset.size),
            );
        }
        file.flush()
            .await
            .map_err(|e| AppError::new("asset_write", e.to_string()))?;
        drop(file);

        if downloaded != asset.size {
            return Err(AppError::new(
                "asset_size",
                format!(
                    "{} is incomplete (received {downloaded} of {} bytes).",
                    asset.filename, asset.size
                ),
            ));
        }

        self.emit_progress(
            DownloadPhase::Verifying,
            &asset.id,
            downloaded,
            Some(asset.size),
        );
        let actual = self.sha256(partial.clone(), generation).await?;
        if actual != asset.sha256.to_lowercase() {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(AppError::new(
                "checksum_mismatch",
                format!("Checksum verification failed for {}.", asset.filename),
            ));
        }
        self.emit_progress(
            DownloadPhase::Installing,
            &asset.id,
            downloaded,
            Some(asset.size),
        );
        install_verified_asset(&partial, &destination).await?;
        make_executable_if_runtime(&destination, &asset.kind)?;
        Ok(())
    }

    pub fn cancel_download(&self) {
        self.cancellation.cancel();
    }

    async fn write_engine_version(&self, version: &str) -> AppResult<()> {
        let path = self.install_dir.join(ENGINE_VERSION_MARKER);
        tokio::fs::write(path, version.trim())
            .await
            .map_err(|error| AppError::new("asset_install", error.to_string()))
    }

    fn emit_progress(&self, phase: DownloadPhase, asset: &str, received: u64, total: Option<u64>) {
        let percent = total
            .filter(|total| *total > 0)
            .map(|total| (received as f64 / total as f64 * 100.0).min(100.0));
        let _ = self.app.emit(
            "voice://download-progress",
            DownloadProgress {
                phase,
                asset: asset.to_owned(),
                received,
                total,
                percent,
            },
        );
    }

    fn ensure_not_cancelled(&self, generation: u64) -> AppResult<()> {
        if self.cancellation.is_cancelled(generation) {
            Err(download_cancelled())
        } else {
            Ok(())
        }
    }

    async fn sha256(&self, path: PathBuf, generation: u64) -> AppResult<String> {
        self.cancellation
            .run(generation, sha256_file(path))
            .await
            .ok_or_else(download_cancelled)?
    }
}

/// Development builds use the locally-built runtime from `<workspace>/bin`; write the same
/// compatibility marker that release installation writes so a verified local runtime is usable.
fn ensure_development_engine_marker() -> AppResult<()> {
    #[cfg(debug_assertions)]
    {
        for root in development_roots() {
            let directory = root.join("bin");
            let server = directory.join(SERVER_FILENAME);
            let marker = directory.join(ENGINE_VERSION_MARKER);
            if server.is_file() && !engine_version_matches(&server, REQUIRED_ENGINE_VERSION) {
                fs::write(marker, REQUIRED_ENGINE_VERSION)
                    .map_err(|error| AppError::new("asset_marker", error.to_string()))?;
            }
        }
    }
    Ok(())
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

fn download_cancelled() -> AppError {
    AppError::new(
        "download_cancelled",
        "Asset download was cancelled and can be resumed later.",
    )
}

fn validate_https_url(value: &str, label: &str) -> AppResult<url::Url> {
    let parsed =
        url::Url::parse(value).map_err(|error| AppError::new("manifest_url", error.to_string()))?;
    if parsed.scheme() != "https" {
        return Err(AppError::new(
            "manifest_url",
            format!("{label} must use HTTPS."),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::new(
            "manifest_url",
            format!("{label} cannot contain embedded credentials."),
        ));
    }
    Ok(parsed)
}

fn select_asset_set<'a>(
    manifest: &'a ReleaseManifest,
    platform: &str,
    arch: &str,
    backend_id: &str,
) -> AppResult<[&'a ReleaseAsset; 2]> {
    let matching = |kind: &str| {
        manifest
            .assets
            .iter()
            .filter(|asset| {
                asset.kind == kind
                    && (asset.platform == "all" || asset.platform == platform)
                    && (asset.arch == "all" || asset.arch == arch)
            })
            .collect::<Vec<_>>()
    };
    let runtimes = matching("runtime")
        .into_iter()
        .filter(|asset| asset.backend_id.is_none())
        .collect::<Vec<_>>();
    let model_assets = matching("model")
        .into_iter()
        .filter(|asset| asset.backend_id.as_deref() == Some(backend_id))
        .collect::<Vec<_>>();
    if runtimes.len() != 1 || model_assets.len() != 1 {
        return Err(AppError::new(
            "asset_unsupported",
            format!(
                "Expected one runtime and one {backend_id} model for {platform}/{arch}; found {} and {}.",
                runtimes.len(),
                model_assets.len()
            ),
        ));
    }
    let expected_model = models::backend(backend_id)
        .ok_or_else(|| AppError::new("unsupported_backend", "Unsupported backend."))?
        .model_filename;
    if runtimes[0].filename != SERVER_FILENAME || model_assets[0].filename != expected_model {
        return Err(AppError::new(
            "manifest_filename",
            "The manifest filenames do not match the supported runtime and model.",
        ));
    }
    Ok([runtimes[0], model_assets[0]])
}

fn engine_version_matches(server: &Path, required: &str) -> bool {
    server.parent().is_some_and(|directory| {
        fs::read_to_string(directory.join(ENGINE_VERSION_MARKER))
            .is_ok_and(|installed| installed.trim() == required)
    })
}

fn backup_path(destination: &Path) -> PathBuf {
    let extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!("{value}.old"))
        .unwrap_or_else(|| "old".into());
    destination.with_extension(extension)
}

fn recover_interrupted_install(destination: &Path) -> AppResult<()> {
    let backup = backup_path(destination);
    if !backup.exists() {
        return Ok(());
    }
    if destination.exists() {
        fs::remove_file(&backup).map_err(|e| AppError::fatal("asset_recover", e.to_string()))?;
    } else {
        fs::rename(&backup, destination)
            .map_err(|e| AppError::fatal("asset_recover", e.to_string()))?;
    }
    Ok(())
}

async fn install_verified_asset(partial: &Path, destination: &Path) -> AppResult<()> {
    let partial = partial.to_owned();
    let destination = destination.to_owned();
    tokio::task::spawn_blocking(move || {
        recover_interrupted_install(&destination)?;
        let backup = backup_path(&destination);
        if destination.exists() {
            fs::rename(&destination, &backup)
                .map_err(|e| AppError::new("asset_install", e.to_string()))?;
        }
        if let Err(error) = fs::rename(&partial, &destination) {
            if backup.exists() {
                let _ = fs::rename(&backup, &destination);
            }
            return Err(AppError::new("asset_install", error.to_string()));
        }
        if backup.exists() {
            let _ = fs::remove_file(backup);
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::new("asset_install", e.to_string()))?
}

fn validate_manifest(manifest: &ReleaseManifest) -> AppResult<()> {
    if manifest.schema_version != 2 {
        return Err(AppError::new(
            "manifest_version",
            format!(
                "Unsupported release manifest version {}.",
                manifest.schema_version
            ),
        ));
    }
    if manifest.release.trim().is_empty() || manifest.engine_version.trim().is_empty() {
        return Err(AppError::new(
            "manifest_metadata",
            "Release and engine versions must be present.",
        ));
    }
    validate_https_url(&manifest.license_notice_url, "License notice URL")?;
    let mut ids = std::collections::HashSet::new();
    for asset in &manifest.assets {
        if asset.id.trim().is_empty() || !ids.insert(asset.id.as_str()) {
            return Err(AppError::new(
                "manifest_asset_id",
                "Release asset IDs must be non-empty and unique.",
            ));
        }
        if asset.size == 0 {
            return Err(AppError::new(
                "manifest_size",
                format!("{} has an invalid size.", asset.id),
            ));
        }
        if asset.sha256.len() != 64 || hex::decode(&asset.sha256).is_err() {
            return Err(AppError::new(
                "manifest_checksum",
                format!("{} has an invalid SHA-256.", asset.id),
            ));
        }
        validate_https_url(&asset.url, "Release asset URL")?;
        match asset.kind.as_str() {
            "runtime" if asset.backend_id.is_some() => {
                return Err(AppError::new(
                    "manifest_backend",
                    "Shared runtime assets cannot declare a backend ID.",
                ));
            }
            "model" => {
                let Some(backend_id) = asset.backend_id.as_deref() else {
                    return Err(AppError::new(
                        "manifest_backend",
                        "Model assets must declare a backend ID.",
                    ));
                };
                let Some(backend) = models::backend(backend_id) else {
                    return Err(AppError::new(
                        "manifest_backend",
                        format!("{} declares an unsupported backend.", asset.id),
                    ));
                };
                if asset.filename != backend.model_filename {
                    return Err(AppError::new(
                        "manifest_filename",
                        format!("{} does not use the registered model filename.", asset.id),
                    ));
                }
            }
            "runtime" => {}
            _ => {
                return Err(AppError::new(
                    "manifest_kind",
                    format!("{} has an unsupported asset kind.", asset.id),
                ));
            }
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
    fn release_manifest_matches_the_app_version() {
        let manifest: ReleaseManifest =
            serde_json::from_str(include_str!("../assets/release-manifest.json"))
                .expect("release manifest should parse");

        validate_manifest(&manifest).expect("release manifest should be valid");
        #[cfg(windows)]
        select_asset_set(&manifest, "windows", "x86_64", models::NEMOTRON_ID)
            .expect("release manifest should support Windows x64");
        assert_eq!(manifest.release, env!("CARGO_PKG_VERSION"));
        assert!(DEFAULT_RELEASE_MANIFEST_URL.contains(&format!(
            "/v{}/release-manifest.json",
            env!("CARGO_PKG_VERSION")
        )));
    }

    #[test]
    fn rejects_non_https_assets() {
        let manifest = ReleaseManifest {
            schema_version: 2,
            release: "1".into(),
            engine_version: "1".into(),
            license_notice_url: "https://example.com".into(),
            assets: vec![ReleaseAsset {
                id: "bad".into(),
                kind: "model".into(),
                backend_id: Some(models::NEMOTRON_ID.into()),
                platform: "all".into(),
                arch: "all".into(),
                filename: models::NEMOTRON_MODEL_FILENAME.into(),
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
            schema_version: 2,
            release: "1".into(),
            engine_version: "1".into(),
            license_notice_url: "https://example.com".into(),
            assets: vec![ReleaseAsset {
                id: "bad-hash".into(),
                kind: "model".into(),
                backend_id: Some(models::NEMOTRON_ID.into()),
                platform: "all".into(),
                arch: "all".into(),
                filename: models::NEMOTRON_MODEL_FILENAME.into(),
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

    #[test]
    fn rejects_duplicate_asset_ids() {
        let asset = ReleaseAsset {
            id: "duplicate".into(),
            kind: "model".into(),
            backend_id: Some(models::NEMOTRON_ID.into()),
            platform: "all".into(),
            arch: "all".into(),
            filename: models::NEMOTRON_MODEL_FILENAME.into(),
            url: "https://example.com/model".into(),
            size: 1,
            sha256: "0".repeat(64),
        };
        let manifest = ReleaseManifest {
            schema_version: 2,
            release: "1".into(),
            engine_version: "1".into(),
            license_notice_url: "https://example.com/license".into(),
            assets: vec![asset.clone(), asset],
        };

        assert_eq!(
            validate_manifest(&manifest)
                .expect_err("duplicate IDs")
                .code,
            "manifest_asset_id"
        );
    }

    #[test]
    fn requires_exact_supported_asset_filenames() {
        let manifest = ReleaseManifest {
            schema_version: 2,
            release: "1".into(),
            engine_version: "1".into(),
            license_notice_url: "https://example.com/license".into(),
            assets: vec![
                ReleaseAsset {
                    id: "runtime".into(),
                    kind: "runtime".into(),
                    backend_id: None,
                    platform: "all".into(),
                    arch: "all".into(),
                    filename: "unexpected-runtime".into(),
                    url: "https://example.com/runtime".into(),
                    size: 1,
                    sha256: "0".repeat(64),
                },
                ReleaseAsset {
                    id: "model".into(),
                    kind: "model".into(),
                    backend_id: Some(models::NEMOTRON_ID.into()),
                    platform: "all".into(),
                    arch: "all".into(),
                    filename: models::NEMOTRON_MODEL_FILENAME.into(),
                    url: "https://example.com/model".into(),
                    size: 1,
                    sha256: "0".repeat(64),
                },
            ],
        };

        assert_eq!(
            select_asset_set(
                &manifest,
                std::env::consts::OS,
                std::env::consts::ARCH,
                models::NEMOTRON_ID,
            )
            .expect_err("unexpected filename")
            .code,
            "manifest_filename"
        );
    }

    #[test]
    fn zui_1_engine_marker_cannot_satisfy_zui_2() {
        let directory = tempfile::tempdir().expect("temp directory");
        let server = directory.path().join(SERVER_FILENAME);
        fs::write(&server, b"runtime").expect("runtime");
        fs::write(directory.path().join(ENGINE_VERSION_MARKER), "0.4.0-zui.1").expect("old marker");
        assert!(!engine_version_matches(&server, REQUIRED_ENGINE_VERSION));
        fs::write(
            directory.path().join(ENGINE_VERSION_MARKER),
            REQUIRED_ENGINE_VERSION,
        )
        .expect("new marker");
        assert!(engine_version_matches(&server, REQUIRED_ENGINE_VERSION));
    }

    #[test]
    fn selects_the_nemotron_model() {
        let runtime = ReleaseAsset {
            id: "runtime".into(),
            kind: "runtime".into(),
            backend_id: None,
            platform: "all".into(),
            arch: "all".into(),
            filename: SERVER_FILENAME.into(),
            url: "https://example.com/runtime".into(),
            size: 1,
            sha256: "0".repeat(64),
        };
        let model = |id: &str, filename: &str| ReleaseAsset {
            id: format!("model-{id}"),
            kind: "model".into(),
            backend_id: Some(id.into()),
            platform: "all".into(),
            arch: "all".into(),
            filename: filename.into(),
            url: format!("https://example.com/{filename}"),
            size: 1,
            sha256: "0".repeat(64),
        };
        let manifest = ReleaseManifest {
            schema_version: 2,
            release: "1".into(),
            engine_version: "0.4.0-zui.1".into(),
            license_notice_url: "https://example.com/license".into(),
            assets: vec![
                runtime,
                model(models::NEMOTRON_ID, models::NEMOTRON_MODEL_FILENAME),
            ],
        };

        let selected = select_asset_set(&manifest, "windows", "x86_64", models::NEMOTRON_ID)
            .expect("Nemotron asset set");
        assert_eq!(selected[1].backend_id.as_deref(), Some(models::NEMOTRON_ID));
    }
}
