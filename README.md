# Bard

**Capture. Dictate. Keep.**

Bard is a minimal native Windows workspace that merges two things into one
keyboard-first panel: an advanced clipboard manager and local voice dictation
— inspired by shadcn's Copper and built as something Apple would ship for
Windows.

- **Capture** — double-tap `Shift` to grab a selection (text or images),
  and everything you copy anywhere lands in a searchable timeline. Sections,
  favorites, multi-select, inline edit, mixed notes (images attached to
  text), Markdown export.
- **Dictate** — tap the same `Ctrl` twice to start dictation, or hold the second
  press for push-to-talk. A short tap locks recording so you can speak hands-free;
  another `Ctrl` `Ctrl` stops it. Bard uses a local `transcribe-cpp` GGUF model:
  choose Bard Fast, Bard English, or Bard Multilingual in **Settings → Models**.
  Audio capture and local transcription stay on this device. Optional text-only
  post-processing runs only after local transcription when enabled in
  **Post-processing**; it sends only the finished transcript to OpenRouter and
  otherwise uses the local result. The text pastes into the app you're using, or
  lands in the timeline when the panel is open. Esc cancels; the model unloads
  after five idle minutes. A single take can run for up to 20 minutes. In
  **Settings → Dictation**, the recording pill can sit at the top or bottom of
  the screen and at the left, middle, or right.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Capture selection | `Shift` `Shift` (or `Ctrl+Shift+C`) |
| Toggle panel | `Shift` `Shift` (right) or `Ctrl+Shift+Space` |
| Dictate | same-key `Ctrl` `Ctrl` (hold the second press for push-to-talk) |
| Copy item | `Enter` |
| Favorite | `Space` |
| Delete | `Del` |
| Switch section | `Ctrl+K` |
| Multi-select | `Ctrl`+click, `Shift`+click / `Shift`+arrows |
| Edit item | double-click |
| Hide panel | `Esc` (steps: clear search → clear selection → hide) |

Capture and panel chords are rebindable in Settings. The same-key double-tap gestures are fixed so they remain available everywhere.

## Building

Prerequisites: Rust (stable MSVC), Node/Bun, CMake, MSVC Build Tools (C++
workload), WebView2 (preinstalled on Windows 10/11).

```sh
bun install
bun run tauri dev      # develop
bun run tauri build    # release + MSI & NSIS installers
```

The three model roles are pinned to the immutable
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
  same-key Shift and Ctrl gestures, transactional OS shortcut registration, and
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
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

The exact model release gate is documented in `docs/BUILD.md`. Set
`BARD_SMOKE_MODELS` (and `BARD_SMOKE_WAV` for transcription) only when running
against the staged model files; the smoke tests are no-ops otherwise. The
OpenRouter pane is optional and should be exercised with a disposable key
entered in the UI, never pasted into chat or committed to the repository.

Idle cost is near zero: a 300 ms sequence poll, an event-driven input hook,
and no model in memory until the first dictation.

Idle cost is near zero: a 300 ms sequence poll, an event-driven input hook,
and no model in memory until the first dictation.