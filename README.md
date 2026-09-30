# Bard

**Clipboard, Draw and Dictate.**

Bard is a minimal native Windows workspace that merges three things into one
keyboard-first panel: an advanced clipboard manager, a sketchpad and local voice dictation
— inspired by shadcn's Copper and built as something Apple would ship for
Windows.

- **Capture** — double-tap `Left Shift` to grab a selection (text or images),
  and everything you copy anywhere lands in a searchable timeline. Sections,
  favorites, multi-select, inline edit, mixed notes (images attached to
  text), Markdown export.
- **Dictate** — tap the same `Ctrl` twice to start dictation, or hold the second
  press for push-to-talk. A short tap locks recording so you can speak hands-free;
  another `Ctrl` `Ctrl` stops it. Bard uses a local `transcribe-cpp` GGUF model:
  choose Bard Fast, Bard English, or Bard Multilingual in **Settings → Models**.
  Audio capture and local transcription stay on this device. Optional text-only
  Refine runs only after local transcription when switched on in
  **Settings → Refine**; it sends only the finished transcript to OpenRouter
  and otherwise uses the local result. The text pastes into the app you're using, or
  is retained in the local timeline according to the Dictation lifespan setting. Esc cancels; the model unloads
  after five idle minutes. A single take can run for up to 20 minutes. In
  **Settings → Dictation**, the recording pill can sit at the top or bottom of
  the screen and at the left, middle, or right.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Capture selection | `Left Shift` `Left Shift` |
| Paste item 1 / 2 / 3 | `Right Shift` `Right Shift` then `1` / `2` / `3` |
| Toggle panel | `Right Shift` `Right Shift` |
| Settings | `Right Alt` `Right Alt` |
| Draw | `Left Alt` `Left Alt` |
| Dictate | same-key `Ctrl` `Ctrl` (hold the second press for push-to-talk) |
| Paste into the app you came from | `Enter` |
| Paste item 1–9 | `Ctrl+1` … `Ctrl+9` |
| Copy item | `Ctrl+C` |
| Mark done | `Space` |
| Delete | `Del` |
| Switch section | `Ctrl+K` |
| Multi-select | `Ctrl`+click, `Shift`+click / `Shift`+arrows |
| Edit item | double-click |
| Hide panel | `Esc` (steps: clear search → clear selection → hide) |

Shortcuts are fixed. The same-key gestures and the Right Shift item-paste sequence remain available from every app; the Settings → Shortcuts page is a reference, not a key editor.

## Licence

**One payment. Nothing renews, and there is no subscription and no account.**

Bard includes a 14-day local trial that needs no account and no key. A licence
costs **US$20 once** and runs for **365 days from the day you first activate
it**, on up to **three** activated Windows computers. When those 365 days are
over the licence ends: nothing is charged again, there is no renewal step, and
there is no account to sign into. Buy again to carry on.

The raw key is stored in Windows Credential Manager, and Bard keeps a signed
offline lease so a network outage does not discard a valid entitlement. When
the paid term ends, existing captures stay readable and exportable while new
capture, dictation, drawing, and editing are paused.

## Building

Prerequisites: Rust (stable MSVC), Node/Bun, CMake, MSVC Build Tools (C++
workload), and WebView2 (present on current Windows 10/11 installations; the
installer can bootstrap it when a clean machine needs it).

```sh
bun install
bun run tauri dev      # develop
bun run build:msix     # Store MSIX validation package
```

A local release — the NSIS installer plus its updater signature — is built by
`scripts/build-release.ps1`, not by a bare `tauri build`:

```powershell
.\scripts\build-release.ps1 -Unsigned   # ships unsigned, by decision
```

Bard ships in two ways, and only one of them is signed.

- **Direct download from miiso.dev — an NSIS installer that is deliberately
  unsigned.** No publicly trusted code-signing certificate is bought for Bard,
  so the installer is not Authenticode-signed and Windows shows a SmartScreen
  "Windows protected your PC" prompt the first time you run it. The prompt is
  the expected result, not a corrupt download. Click **More info →
  Run anyway**, or install from the Microsoft Store instead and skip it. Every
  other protection is intact and independently checked: the installer carries
  an Ed25519 updater signature, `scripts/verify-release.ps1` verifies that
  signature against the public key embedded in the app before anything is
  published, and the Store copy is signed by Microsoft. The signing path for a
  future certificate is still in the script and the runbook, unused.
- **Microsoft Store — an MSIX package signed by Microsoft** after
  certification. There is no downloadable MSIX: a sideloaded copy would carry a
  different identity from the Store copy, install beside it, and never receive
  an update. The package takes its identity from Partner Center.


The three model roles are pinned to the hash-verified
`https://github.com/miisodev/bard-models` release `models-v2`:
`whisper-small-Q6_K.gguf` for Fast multilingual,
`parakeet-tdt-0.6b-v2-Q6_K.gguf` for Best English, and
`Qwen3-ASR-1.7B-Q6_K.gguf` for Best multilingual. The first-party release
rehosts the exact listed bytes with model-specific notices. The embedded
catalog records the conversion provenance, model-specific licence, exact size,
and SHA-256; downloads resume, hash the complete file, and load through
`transcribe-cpp` before activation.

## Architecture

Tauri 2 (Rust + React/TS/Vite), SQLite as the single source of truth:

- `src-tauri/src/capture.rs` — one rdev raw-input coordinator for the fixed
  same-key Shift, Alt, and Ctrl gestures, the Right Shift item-paste slot, and
  the enigo copy chord / zero-width-sentinel clipboard dance
- `src-tauri/src/monitor.rs` — clipboard sequence poller, privacy-format aware,
  dedup by content
- `src-tauri/src/dictation/` — cpal recording (device-selectable), rubato
  resampling to 16 kHz, earshot VAD (pure Rust), the sole `transcribe-cpp`
  GGML inference engine, and an overlay window with cancel + live levels
- `src/` — the panel: sectioned timeline, search, favorites, multi-select,
  context menu, drag-drop, inline edit, settings, onboarding

## Verification

```sh
bun run build
bun audit
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo audit --file src-tauri/Cargo.lock   # cargo install cargo-audit
```

`cargo audit` is the Rust half of the dependency gate and reads
`src-tauri/Cargo.lock` explicitly — there is no `Cargo.lock` at the repository
root, so a bare `cargo audit` looks at the wrong place. As of 2026-09-26 it
reports 0 vulnerabilities and 8 warnings over 682 locked packages; three of those
crates (`glib`, `memmap2`, `proc-macro-error`) are in the lockfile but are never
compiled for Windows, which
`cargo tree --target x86_64-pc-windows-msvc -i <crate>` will confirm. CI runs the
same command through `rustsec/audit-check`, pinned to a commit; a critical
advisory fails the job and an informational one is reported on a Check.

The model release gate is `cargo test smoke_test_exact_models_when_requested` against the staged files. Set
`BARD_SMOKE_MODELS` (and `BARD_SMOKE_WAV` for transcription) only when running
against the staged model files; the smoke tests are no-ops otherwise. The
OpenRouter pane is optional and should be exercised with a disposable key
entered in the UI, never pasted into chat or committed to the repository.

Idle cost is near zero: a 300 ms sequence poll, an event-driven input hook,
and no model in memory until the first dictation.