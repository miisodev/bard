# Bard

**Clipboard, Draw and Dictate.** A small desktop companion for Windows and Linux
that keeps what you copy, lets you say or sketch something new, and hands
everything back on the clipboard.

Bard only ever works with the one thing on your clipboard: it keeps it, shows
it, swaps it for something you kept earlier, or adds to it by voice. You still
copy and paste the way you always have. Bard never types into another
application.

It is free and open source. Bard is [Miiso](https://miiso.dev)'s idea and his to
maintain; version 4 was redesigned and rebuilt by Claude, Anthropic's Fable
model, which he handed the project to.

## What it does

- **Keeps what you copy.** Text, links, colours, images and files, from any
  application, in one searchable history. Copies a password manager marks as
  private are never kept. A soft sound and a small badge at the top of the
  screen confirm each one; turn either off.
- **Three slots, one keystroke.** Your pinned items, then your most recent,
  fill slots 1 to 3. Put any of them back on the clipboard from anywhere,
  or cycle through them and watch the badge show where you are.
- **Shows you the clipboard.** One shortcut reads back what is on it right now.
- **Dictates on your own machine.** Hold a key and talk, or tap to start and tap
  to stop. A local speech model turns it into text, reads it back to you, and
  puts it on the clipboard. Audio never leaves the computer.
- **Draws.** Pen, arrows, shapes and text on a page of its own; copy it as an
  image or save it as SVG.
- **Stays out of the way.** Bard lives in the tray. Its windows come when you
  call them and tuck away when you click elsewhere.

## Shortcuts

Every shortcut can be changed in **Settings → Shortcuts**. On Wayland the
desktop owns them and asks once. A desktop that already knows Bard from
version 3 keeps the keys it assigned then, so the table below may not match;
Settings → Shortcuts shows the keys actually bound, names any your desktop
keeps for itself, and opens the desktop's own dialog to change them.

| Action | Default |
| --- | --- |
| Show or hide Bard | `Alt+Shift+V` |
| Dictate: hold to talk, tap to toggle | `Alt+Shift+Space` |
| Copy slot 1 / 2 / 3 | `Alt+Shift+1` / `2` / `3` |
| Cycle through the slots | `Alt+Shift+C` |
| Show what is on the clipboard | `Alt+Shift+P` |
| Pause or resume keeping copies | `Alt+Shift+X` |
| Open Draw | `Alt+Shift+D` |
| Open Settings | `Alt+Shift+,` |
| Show the shortcut guide | `Alt+Shift+/` |

In the main window: `↑` `↓` move, `Enter` copies and closes, `Shift+Enter`
copies and stays, `Ctrl+P` pins, `Ctrl+S` keeps forever, `Del` deletes,
`F2` edits, `Ctrl+F` searches, and `Esc` steps back.

## Privacy

Bard has no account, no telemetry and no sync. Your history, your audio and
your drawings stay on your computer. Bard uses the network for two things only:
downloading a speech model when you ask for one, and checking for updates. The
source is here so you can check that for yourself.

## Speech models

Bard offers two models and runs nothing else. Each is an open-weight model,
converted to GGUF, hosted at
[miisodev/bard-models](https://github.com/miisodev/bard-models) and pinned in
the app by exact size and SHA-256; a download is hashed and loaded before it is
ever used.

- **Bard Fast**: `parakeet-tdt-0.6b-v3`, about 580 MB. The place to start.
- **Bard General**: `Qwen3-ASR-1.7B`, about 1.6 GB. More accurate.

The model unloads after five idle minutes.

## Install

Downloads for Windows (installer) and Linux (`.deb`, `.rpm`) are at
[miiso.dev/bard](https://miiso.dev/bard) and on the
[releases page](https://github.com/miisodev/bard-releases/releases), with
checksums.

The Windows installer is not Authenticode-signed, so SmartScreen shows a
"Windows protected your PC" prompt the first time; choose **More info → Run
anyway**. Updates are verified against an Ed25519 key built into the app.

## Licence

[MIT](LICENSE).

## Building

Prerequisites: Rust (the pinned toolchain in `rust-toolchain.toml`), Bun, and
on Linux the GTK 3 / WebKitGTK 4.1 development packages; on Windows, MSVC
Build Tools, CMake and WebView2 (present on current Windows 10/11
installations; the installer can bootstrap it when a clean machine needs it).

```sh
bun install --frozen-lockfile
bun run tauri dev      # develop
```

A local release — the NSIS installer plus its updater signature — is built by
`scripts/build-release.ps1`, not by a bare `tauri build`:

```powershell
.\scripts\build-release.ps1 -Unsigned   # ships unsigned, by decision
```

Linux packages are built in the pinned `ubuntu:22.04` container, because a
build on a newer distribution demands a glibc its users do not have.

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
