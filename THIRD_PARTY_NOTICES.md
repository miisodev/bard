# Bard Third-Party Notices

Bard is proprietary software distributed under the Bard licence. It does not
vendor code from the Cooper or Handy reference applications. The following
components are used or offered by the product and retain their own notices.

## Runtime and UI

- Tauri 2 and wry — MIT / Apache-2.0
- React and React DOM — MIT
- Lucide — ISC
- Inter — SIL Open Font License 1.1
- meodai/color-names — MIT (bundled "Best of" colour-name dataset, © David Aerne)
- Microsoft Edge WebView2 — Microsoft runtime licence
- Microsoft Windows Credential Manager — Windows platform component
- rusqlite — MIT
- reqwest, rustls, webpki-roots, and related Rust dependencies — MIT / Apache-2.0
- ed25519-dalek and sha2 — BSD-3-Clause / Apache-2.0
- keyring — MIT / Apache-2.0
- transcribe-cpp / ggml — upstream project licences; the exact source and
  dependency lock are retained in the release source archive

The complete dependency licence and SBOM report is generated with the release
candidate and published alongside the installer. A source archive containing
the exact Cargo.lock and build inputs accompanies the release notices.

## Bundled and downloadable transcription models

Model files are not copied into the Bard installer. They are downloaded only
after the user chooses a model, and their catalogue records the exact source,
revision, byte size, SHA-256, and upstream licence.

- **Bard Fast** — Whisper Small, `openai/whisper-small`, Apache-2.0. The
  conversion source is `handy-computer/whisper-small-gguf`; Bard's pinned
  bytes are hosted at `miisodev/bard-models` release `models-v2`.
- **Bard English** — Parakeet TDT 0.6B v2, `nvidia/parakeet-tdt-0.6b-v2`,
  CC-BY-4.0. Attribution: NVIDIA Corporation and the Parakeet contributors.
- **Bard Multilingual** — Qwen3 ASR, `Qwen/Qwen3-ASR`, Apache-2.0. The
  conversion source and exact revision are recorded in the catalogue.

Model notices are displayed in **Settings → Models** before a model is used.
Users must retain the applicable attribution when redistributing a downloaded
model or derived work.

## Optional services

- **OpenRouter** — optional, user-enabled text-only post-processing. Bard
  sends the finished transcript text and selected model identifier, not audio,
  images, or clipboard history.
- **Polar** — checkout, annual subscription billing, licence-key issuance, and
  customer portal.
- **Hugging Face and GitHub** — model and installer file delivery. The desktop
  client does not upload captures or transcripts to either service.

## Reporting

A suspected licence or attribution problem should be sent to
`legal@miiso.dev` with the component, version, and reproduction details.
