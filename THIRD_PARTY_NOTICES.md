# Bard Third-Party Notices

Bard is open source under the MIT licence. It does not
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
candidate and published alongside the installer. The exact `Cargo.lock` and the
build inputs are recorded in the published SBOM and provenance document, which
are generated from the same release commit; no source archive is published with
the release.

## Bundled and downloadable transcription models

Model files are not copied into the Bard installer. They are downloaded only
after the user chooses a model, and their catalogue records the exact source,
revision, byte size, SHA-256, and upstream licence.

- **Bard Fast** — Parakeet TDT 0.6B v3, `nvidia/parakeet-tdt-0.6b-v3`,
  CC-BY-4.0. Attribution: NVIDIA Corporation and the Parakeet contributors.
  The conversion source is `handy-computer/parakeet-tdt-0.6b-v3-gguf`, and
  Bard's pinned bytes are the Q6_K quantisation hosted at
  `miisodev/bard-models` release `models-v3`.
- **Bard General** — Qwen3 ASR, `Qwen/Qwen3-ASR-1.7B`, Apache-2.0. The
  conversion source and exact revision are recorded in the catalogue.

Model notices are displayed in **Settings → Models** before a model is used.
Users must retain the applicable attribution when redistributing a downloaded
model or derived work.

## Optional services

- **Hugging Face and GitHub** — model and installer file delivery. The desktop
  client does not upload captures or transcripts to either service.

## Reporting

A suspected licence or attribution problem should be sent to
`legal@miiso.dev` with the component, version, and reproduction details.
