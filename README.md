# Zui. Voice

Zui. Voice is a local-first, push-to-talk Vietnamese dictation app. Hold **Right Alt**, speak, and release to insert the transcript into the app that was focused when recording began.

## Development

Prerequisites:

- Node.js 20+
- Rust stable with the platform's Tauri prerequisites
- The development assets in `bin/`:
  - `parakeet-server.exe` (or `parakeet-server` on Unix)
  - `parakeet-ctc-0.6b-Vietnamese-q8_0.gguf`

```powershell
npm install
npm run tauri dev
```

The app records 16 kHz mono WAV audio only while the hold key is down. Audio is deleted immediately after transcription and transcript history is never stored.

## Architecture

The React UI receives typed state and spectrum events from a Rust controller. Native responsibilities are isolated behind services:

- `AudioRecorder`: microphone capture, downmixing, resampling, silence rejection, WAV creation.
- `TranscriptionBackend`: replaceable backend contract. `ParakeetBackend` supervises the local OpenAI-compatible server; a future `WhisperCppBackend` can use the same contract.
- `HotkeyService`: global press/release events with a Windows low-level hook and portable desktop fallback.
- `ClipboardService`: common-format snapshot, guarded paste, and race-safe restore.
- `platform`: foreground-target validation and caret/pointer overlay placement.
- `AssetManager`: release manifest, resumable downloads, SHA-256 verification, and atomic install.

## Release assets

Set `ZUI_RELEASE_MANIFEST_URL` at build time to a public HTTPS JSON manifest following `src-tauri/assets/release-manifest.example.json`. Every asset is selected by OS/architecture and verified before installation. Do not publish the supplied GGUF until its exact provenance and redistribution terms have been verified.

## Platform notes

- Windows 10/11 x64 is the primary locally verified target.
- macOS requires Microphone and Accessibility permissions.
- X11 supports global key observation and synthetic paste subject to desktop security policy.
- Wayland intentionally falls back to copying the transcript for manual paste. A portal-configured `Ctrl+Alt+Space` chord is the intended packaged integration.

## Privacy

The only network operation is the explicit first-run download of versioned, checksummed assets. Transcription is sent only to a loopback server started by Zui. Voice. Logs must not include audio bytes or transcript text.
