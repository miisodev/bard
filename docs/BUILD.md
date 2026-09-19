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

## One transcription engine

`transcribe-cpp` is the only engine bard links. It loads GGUF/ggml models
through ggml and covers every architecture in the catalogue — whisper,
parakeet, moonshine, canary, sensevoice, gigaam, qwen3, voxtral. **A model is
always a single file at a path.**

**There is no ONNX runtime in the tree, and adding one is not a free
dependency.** `transcribe-rs` was briefly linked here on the belief that
non-whisper architectures needed it; they do not — `transcribe-cpp` ships
`parakeet.h` and `moonshine_streaming.h`, and every model Handy offers is GGUF.
It pulled in `ort`, which fetches pyke's prebuilt ONNX Runtime, which executes
AVX2 in a static initialiser and crashes at process start on any pre-Haswell
CPU. That could not be turned off from bard's manifest, because the dependency
declared `ort` with default features and Cargo features are additive.

**Loading failures are about the conversion, not the family.** A model that
will not open is almost always a GGUF laid out in a way the engine does not
read, not an unsupported architecture. Third-party conversions of the same
model are not interchangeable — which is why catalogue entries pin a revision
and carry a checksum.

**Acceleration is off by default.** `transcribe-cpp` reaches a Vulkan GPU
backend and per-ISA CPU dispatch through non-default features, and those make
`transcribe-cpp-sys` build ggml from source — which is what needs CMake on
PATH and the short symlink above.
