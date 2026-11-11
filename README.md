# Bard

**Clipboard, Draw and Dictate.**

Bard is a native Windows and Linux app that merges three things into one
keyboard-first panel: an advanced clipboard manager, a sketchpad and local
voice dictation — inspired by shadcn's Copper and built as something Apple
would ship for Windows.

Everything Bard does ends on your clipboard. Bard never types into another
application on your behalf: you pick it, dictate it or sketch it, and you
paste it yourself. That is the promise, and it is why focus, timing and
per-application permissions never decide whether your words arrive.

- **Clipboard** — every copy and cut, from any application, on Windows, X11 and
  Wayland, kept in one searchable timeline. Text, links, colours, images, files
  and drawings. Pin the three you reuse most and put any of them back on the
  clipboard with one chord, from any application, without opening a window.
- **Dictate** — hold one key to talk, or tap it to start and tap again to stop.
  Transcription runs on your own machine through a local `transcribe-cpp` GGUF
  model: choose **Bard Fast** or **Bard General** in **Settings → Models**.
  Audio capture and local transcription never leave the device. Every transcript is also kept
  in Bard's history, so a later copy cannot lose it. The model unloads after
  five idle minutes and a single take can run for up to 20 minutes.
- **Draw** — pen, arrows, shapes and text on a page of its own. Copy the page
  as an image in one keystroke, or save it as an SVG.

## Global shortcuts

One table of shortcuts, the same on every platform, and every one of them can
be changed in **Settings → Shortcuts** (except on Wayland, where the desktop
owns the bindings and asks once).

| Action | Default |
| --- | --- |
| Show or hide Bard | `Alt+Shift+V` |
| Dictate — hold to talk, tap to toggle | `Alt+Shift+Space` |
| Copy item 1 / 2 / 3 | `Alt+Shift+1` / `2` / `3` |
| Cycle through items 1–3 | `Alt+Shift+C` |
| Preview what is on the clipboard | `Alt+Shift+P` |
| Pause or resume keeping what you copy | `Alt+Shift+X` |
| Open Draw | `Alt+Shift+D` |
| Open Settings | `Alt+Shift+,` |
| Show the shortcut guide | `Alt+Shift+/` |

Inside the panel: `↑` `↓` move, `Enter` copies and closes, `Shift+Enter`
copies and stays, `Ctrl+P` pins, `Ctrl+S` keeps forever, `Del` deletes,
`Ctrl+F` searches, `Esc` steps back through search, selection, then the
window, and the context-menu key opens the row's menu.

## Licence

Bard is free and open source under the [MIT licence](LICENSE). There is no
account, no key, no trial and no telemetry. A tool that reads everything you
copy should be one you can read back.

## Building

Prerequisites: Rust (the pinned toolchain in `rust-toolchain.toml`), Bun, and
on Linux the GTK 3 / WebKitGTK 4.1 development packages; on Windows, MSVC
Build Tools, CMake and WebView2 (present on current Windows 10/11
installations; the installer can bootstrap it when a clean machine needs it).

```sh
bun install --frozen-lockfile
bun run tauri dev      # develop
bun run build:msix     # Store MSIX validation package (Windows)
```

A local release — the NSIS installer plus its updater signature — is built by
`scripts/build-release.ps1`, not by a bare `tauri build`:

```powershell
.\scripts\build-release.ps1 -Unsigned   # ships unsigned, by decision
```

Linux packages are built in the pinned `ubuntu:22.04` container, because a
build on a newer distribution demands a glibc its users do not have.

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

The two model roles are pinned to hash-verified
`https://github.com/miisodev/bard-models` releases: `parakeet-tdt-0.6b-v3-Q6_K.gguf`
for Fast, from `models-v3`, and `Qwen3-ASR-1.7B-Q6_K.gguf` for General, from
`models-v2`. The first-party release rehosts the exact listed bytes with
model-specific notices. The embedded catalog records the conversion provenance,
model-specific licence, exact size, and SHA-256; downloads resume, hash the
complete file, and load through `transcribe-cpp` before activation.

## Architecture

Tauri 2 (Rust + React/TypeScript/Vite), SQLite as the single source of truth.
The Rust core owns every privileged path; the webview only draws.

- `src-tauri/src/monitor.rs` — clipboard capture. A hidden `AddClipboardFormatListener`
  window on Windows; XFixes selection watching on X11 and XWayland, which is
  how a Wayland copy is heard on GNOME. Privacy-marked copies (password
  managers) are never kept, and Bard's own writes never re-enter its history.
- `src-tauri/src/shortcuts.rs` — the one shortcut table: `RegisterHotKey` on
  Windows, `XGrabKey` on X11, and the GlobalShortcuts portal on Wayland. A
  chord another application already owns fails visibly instead of silently.
- `src-tauri/src/dictation/` — cpal recording (device-selectable), rubato
  resampling to 16 kHz, earshot VAD (pure Rust), and the sole `transcribe-cpp`
  GGML inference engine.
- `src-tauri/src/db.rs` — the schema, the migrations and every query. A change
  to a Rust struct breaks the webview with no compile error, so
  `src/types.ts` mirrors it by hand and both change together.
- `src/` — the panel (`components/Clipboard.tsx`), the settings window, the
  status pill, and the Draw window. No router, no state library, no CSS
  framework: the design tokens in `src/styles/tokens.css` are the whole visual
  language, and `scripts/css-audit.mjs` checks them.

## Verification

```sh
bun run build
bun audit
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo audit --file src-tauri/Cargo.lock   # cargo install cargo-audit
```

`cargo audit` is the Rust half of the dependency gate and reads
`src-tauri/Cargo.lock` explicitly — there is no `Cargo.lock` at the repository
root, so a bare `cargo audit` looks at the wrong place. CI runs the same
command through `rustsec/audit-check`, pinned to a commit; a critical advisory
fails the job and an informational one is reported on a Check.

The model release gate is `cargo test smoke_test_exact_models_when_requested` against the staged files. Set
`BARD_SMOKE_MODELS` (and `BARD_SMOKE_WAV` for transcription) only when running
against the staged model files; the smoke tests are no-ops otherwise.
