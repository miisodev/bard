# Bard

**Capture. Dictate. Keep.**

Bard is a minimal native Windows workspace that merges two things into one
keyboard-first panel: an advanced clipboard manager and offline voice
dictation — inspired by shadcn's Copper and built as something Apple would
ship for Windows.

- **Capture** — double-tap `Shift` to grab a selection (text or images),
  and everything you copy anywhere lands in a searchable timeline. Sections,
  favorites, multi-select, inline edit, mixed notes (images attached to
  text), Markdown export.
- **Dictate** — hold `Ctrl+Space` and speak. A local Whisper model transcribes
  fully offline — nothing leaves your machine. The text pastes into the app
  you're using, or lands in the timeline when the panel is open. Esc cancels
  mid-recording; the model unloads after five idle minutes.

## Keyboard shortcuts

| Action | Shortcut |
| --- | --- |
| Capture selection | `Shift` `Shift` (or `Ctrl+Shift+C`) |
| Toggle panel | `Shift` `Shift` (right) or `Ctrl+Shift+Space` |
| Dictate | hold `Ctrl+Space` |
| Copy item | `Enter` |
| Favorite | `Space` |
| Delete | `Del` |
| Switch section | `Ctrl+K` |
| Multi-select | `Ctrl`+click, `Shift`+click / `Shift`+arrows |
| Edit item | double-click |
| Hide panel | `Esc` (steps: clear search → clear selection → hide) |

All shortcuts are rebindable in Settings.

## Building

Prerequisites: Rust (stable MSVC), Node/Bun, CMake, MSVC Build Tools (C++
workload), WebView2 (preinstalled on Windows 10/11).

```sh
bun install
bun run tauri dev      # develop
bun run tauri build    # release + MSI & NSIS installers
```

Voice models (Whisper Small/Medium/Turbo, SHA-256 pinned) download on first
use from `blob.handy.computer` — the same pinned source the Handy fork ships,
with resume support, cancellation and checksum verification.

## Architecture

Tauri 2 (Rust + React/TS/Vite), SQLite as the single source of truth:

- `src-tauri/src/capture.rs` — rdev double-shift listener + enigo copy chord,
  zero-width-sentinel clipboard dance (from the cooper fork)
- `src-tauri/src/monitor.rs` — clipboard sequence poller, privacy-format aware,
  dedup by content
- `src-tauri/src/dictation/` — cpal recording (device-selectable), rubato
  resampling to 16 kHz, earshot VAD (pure Rust), transcribe-cpp GGML inference,
  overlay window with cancel + live levels
- `src/` — the panel: sectioned timeline, search, favorites, multi-select,
  context menu, drag-drop, inline edit, settings, onboarding

Idle cost is near zero: a 300 ms sequence poll, an event-driven input hook,
and no model in memory until the first dictation.