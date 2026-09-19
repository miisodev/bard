# Building bard

```sh
bun install
bun run tauri dev      # dev server + app
bun run tauri build    # MSI + NSIS installers
bun run build          # tsc + css-audit + vite — the frontend gate
```

## Prerequisites

| Needs | Why |
| --- | --- |
| Rust (MSVC toolchain) | the core |
| Node 18+ / Bun | the webview build |
| Visual Studio Build Tools (VCTools) | MSVC, for the native crates |
| **CMake** | `transcribe-cpp-sys` compiles ggml from source |
| WebView2 | preinstalled on Windows 11 |

## Things that will waste an hour if nobody tells you

**CMake is installed on this machine but is not on PATH.** It lives at
`C:\Program Files\CMake\bin`, and neither PowerShell nor Git Bash finds it by
default. Without it `transcribe-cpp-sys`'s build script fails with a bare
`failed to execute command: program not found`, which names neither the program
nor the crate's real requirement. Put it on PATH before building:

```sh
export PATH="/c/Users/miiso/.cargo/bin:/c/Program Files/CMake/bin:$PATH"
```

**`transcribe-cpp-sys` builds through a symlink under `%LOCALAPPDATA%\tcs`.**
Windows' path limit would otherwise break the ggml build, so the crate points a
short symlink at `target/<profile>/build/transcribe-cpp-sys-*/out/`. If that
target directory does not exist yet the build dies with
`The directory name is invalid. (os error 267)` — a message about a directory
it did not name. Creating the path through the symlink is enough:

```sh
mkdir -p "$LOCALAPPDATA/tcs/<hash>/build"
```

**The dev server must be started detached from the shell that launches it.**
`bun run tauri dev` backgrounded from a Bash tool call dies when that call
ends — Vite goes with it, and the app is left pointing at a dead
`localhost:1420`, showing a "can't reach this page" webview. Start it through
the OS instead:

```powershell
Start-Process bun -ArgumentList 'run','tauri','dev' -WorkingDirectory <repo> -WindowStyle Hidden
```

**Set `BARD_NO_GUARDIAN=1` while developing.** bard spawns a guardian that
relaunches the app if it dies within 20 seconds without leaving a clean-exit
marker (see `src-tauri/src/guardian.rs`). During development that means a killed
app comes back, takes the single-instance lock, and the `tauri dev` you start
next bounces off it and exits 255. `scripts/stop.ps1` stops bard the way the
guardian considers clean; the env var avoids the problem entirely.

**`sed -i` breaks Vite's file watcher.** It replaces a file by rename rather
than writing in place, so Vite keeps serving the previous module while still
logging `hmr update` for every save. Edit in place.

## The two transcription engines

bard links both, because neither covers the field alone:

| Crate | Handles | Model on disk |
| --- | --- | --- |
| `transcribe-cpp` | whisper-family, via ggml | a single `.gguf` / `ggml-*.bin` file |
| `transcribe-rs` (`onnx`) | Parakeet, Moonshine, Canary, SenseVoice, GigaAM, Cohere | a **directory** of ONNX files |

**`ort` downloads a prebuilt ONNX Runtime that must not be shipped.** Its
default features fetch pyke's build, which runs AVX in a static initialiser and
crashes at startup on pre-Haswell CPUs. It cannot be turned off from bard's
manifest, because `transcribe-rs` declares `ort` with default features and
Cargo features are additive. Release builds must supply a baseline ONNX Runtime
via `ORT_LIB_LOCATION`. See `docs/PARITY.md` E0 — this blocks shipping.

That difference is not cosmetic. `ParakeetModel::load(dir, quantization)` takes
a directory, so a model is no longer "a file at a path" — acquisition, storage
and the loader all have to carry which family a model belongs to, because
nothing in the files reliably says. See `docs/PARITY.md` B1 and B5.
