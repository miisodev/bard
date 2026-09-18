# Bard

**Capture. Dictate. Ask.**

Bard is a minimal native Windows workspace that merges three things into one
keyboard-first panel: an advanced clipboard manager, offline voice dictation,
and AI answers — inspired by shadcn's Copper, built as something Apple would
ship for Windows.

- **Capture** — double-tap `Shift` to grab a selection (text or images),
  and everything you copy anywhere lands in a searchable timeline. Favorites,
  delete, copy back with one click.
- **Dictate** — hold `Ctrl+Space` and speak. A local Whisper model transcribes
  fully offline — nothing leaves your machine. The text pastes into the app
  you're using, or lands in the composer when the panel is open.
- **Ask** — type or dictate a question in the composer, optionally attached to
  a captured item as context, and get an answer from any OpenAI-compatible
  provider (bring your own key). The key never leaves your machine.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Capture selection | `Shift` `Shift` (or `Ctrl+Shift+C`) |
| Toggle panel | `Shift` `Shift` (right) or `Ctrl+Shift+Space` |
| Dictate | hold `Ctrl+Space` |
| Copy item | `Enter` (double-click) |

All shortcuts are rebindable in Settings.

## Building

Prerequisites: Rust (stable MSVC), Node/Bun, CMake, MSVC Build Tools (C++
workload), WebView2 (preinstalled on Windows 10/11).

```sh
bun install
bun run tauri dev      # develop
bun run tauri build    # release + MSI & NSIS installers
```

The voice model (~465 MB, Whisper Small, SHA-256 pinned) downloads on first
run from `blob.handy.computer` — the same pinned source the Handy fork ships,
with resume support and checksum verification.

## Architecture

Tauri 2 (Rust + React/TS/Vite), SQLite as the single source of truth:

- `src-tauri/src/capture.rs` — rdev double-shift listener + enigo copy chord,
  zero-width-sentinel clipboard dance (from the cooper fork)
- `src-tauri/src/monitor.rs` — clipboard sequence poller, privacy-format aware,
  dedup by content
- `src-tauri/src/dictation/` — cpal recording, rubato resampling to 16 kHz,
  earshot VAD (pure Rust), transcribe-cpp GGML inference, overlay window
- `src-tauri/src/ask.rs` — OpenAI-compatible chat completions, context assembly
- `src/` — the panel: timeline, search, favorites, composer, settings, onboarding

Idle cost is near zero: a 300 ms sequence poll, an event-driven input hook,
and no model in memory until the first dictation.