import { ExternalLink, HardDrive, LockKeyhole, Scale } from "lucide-react";

export function LegalSettings() {
  return (
    <>
      <section className="settings-section">
        <h2>Privacy</h2>
        <div className="settings-card legal-card">
          <div className="legal-row">
            <LockKeyhole />
            <div>
              <strong>Local transcription</strong>
              <small>Recordings are temporary 16 kHz mono WAV files. They are deleted after success, failure, or cancellation, and transcript history is not stored.</small>
            </div>
          </div>
          <div className="legal-row">
            <HardDrive />
            <div>
              <strong>Network use</strong>
              <small>The explicit first-run engine download uses HTTPS. Dictation is sent only to the loopback speech server running on this computer.</small>
            </div>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2>Third-party software</h2>
        <div className="settings-card legal-card">
          <div className="legal-row">
            <Scale />
            <div>
              <strong>parakeet.cpp runtime</strong>
              <small>Created by mudler/parakeet.cpp and distributed under the MIT License.</small>
              <a href="https://github.com/mudler/parakeet.cpp" target="_blank" rel="noreferrer">Project source <ExternalLink /></a>
            </div>
          </div>
          <div className="legal-row">
            <Scale />
            <div>
              <strong>NVIDIA Nemotron 3.5 ASR</strong>
              <small>The multilingual Q8 model is governed by OpenMDW-1.1 and exposes 32 transcription-ready or broad-coverage locales.</small>
              <a href="https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b" target="_blank" rel="noreferrer">Base model page <ExternalLink /></a>
            </div>
          </div>
        </div>
      </section>

      <p className="legal-footnote">Dependency-specific license texts are collected by the release pipeline. Each dependency remains governed by its own license.</p>
    </>
  );
}
