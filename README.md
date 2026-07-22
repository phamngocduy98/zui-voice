# Zui. Voice

Zui. Voice is a local-first, push-to-talk dictation app powered by multilingual Nemotron 3.5 ASR. Hold your configured key, speak, and release to insert the transcript into the app that was focused when recording began.

## Development

Prerequisites:

- Node.js 20+
- Rust stable with the platform's Tauri prerequisites
- The development assets in `bin/`:
  - `parakeet-server.exe` (or `parakeet-server` on Unix)
  - `nemotron-3.5-asr-streaming-0.6b-q8_0.gguf`

```powershell
npm ci
npm run tauri dev
```

The release runtime is a pinned parakeet.cpp v0.4.0 build with a patch that
forwards the OpenAI-compatible multipart `language` field to Nemotron's language
prompt. Build it on Windows with `./scripts/build-parakeet-runtime.ps1`.

The app records 16 kHz mono WAV audio only while the hold key is down. Audio is deleted immediately after transcription and transcript history is never stored.

## Live subtitles

Live subtitles are an independent, opt-in mode for captions from audio playing on the computer. They always start disabled, retain only bounded PCM/transcript state in memory, keep no history, and pause/drop live audio while push-to-talk dictation has priority. On Windows, CPAL 0.18.1 opens the default render endpoint with WASAPI loopback—never a microphone—and normalizes it to bounded 16 kHz mono PCM. The pinned parakeet.cpp v0.4.0 source has a public streaming C API; the `0.4.0-zui.2` runtime patch exposes it through loopback-only session endpoints while reusing the server's loaded model and inference mutex, so captions are incremental and never write audio files. macOS and Linux remain fail-closed until ScreenCaptureKit/PipeWire transports are validated.

## Architecture

The React UI receives typed state and spectrum events from a Rust controller. Native responsibilities are isolated behind services:

- `AudioRecorder`: microphone capture, downmixing, resampling, silence rejection, WAV creation.
- `TranscriptionBackend`: replaceable backend contract. `ParakeetBackend` supervises the local OpenAI-compatible server and Nemotron GGUF model.
- `HotkeyService`: global press/release events with a Windows low-level hook and portable desktop fallback.
- `ClipboardService`: common-format snapshot, guarded paste, and race-safe restore.
- `platform`: foreground-target validation and caret/pointer overlay placement.
- `AssetManager`: model-aware release manifest, on-demand resumable downloads, SHA-256 verification, and atomic install.

## Release assets

Release builds default to the versioned schema-2 manifest in their matching GitHub release (for example, `v0.2.0/release-manifest.json`). `ZUI_RELEASE_MANIFEST_URL` can override that location at runtime or build time. The runtime and Nemotron model are downloaded from the matching GitHub release and verified. Model and runtime attribution is recorded in `THIRD_PARTY_NOTICES.md`.

## Platform notes

- Windows 10/11 x64 is the primary locally verified target.
- macOS requires Microphone and Accessibility permissions. Until native foreground-window validation is implemented, transcripts are copied for manual paste rather than injected into a potentially different app.
- X11 currently uses the same safe copy-for-manual-paste fallback; global key observation remains subject to desktop security policy.
- Wayland intentionally falls back to copying the transcript for manual paste. A portal-configured `Ctrl+Alt+Space` chord is the intended packaged integration.

## Privacy

The only network operation is the explicit first-run download of versioned, checksummed assets. Transcription is sent only to a loopback server started by Zui. Voice. Logs must not include audio bytes or transcript text.
