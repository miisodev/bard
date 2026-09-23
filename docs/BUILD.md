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
PATH and the short symlink above. If an older checkout left a broken
`target/release/DirectML.dll` reparse point from the removed ONNX path, remove
that generated link before packaging; WiX will otherwise fail while binding the
stale DLL.

## First-party model release gate

The three catalogue assets live in the immutable
[`miisodev/bard-models`](https://github.com/miisodev/bard-models) `models-v2`
release. Before changing a catalogue URL, size, or checksum, stage the exact
bytes and run the engine smoke test:

```powershell
$env:BARD_SMOKE_MODELS = (Get-ChildItem $modelDir -Filter '*.gguf' | ForEach-Object FullName) -join ';'
cargo test --manifest-path src-tauri/Cargo.toml smoke_test_exact_models_when_requested -- --nocapture
Remove-Item Env:BARD_SMOKE_MODELS
```

The test is a no-op unless `BARD_SMOKE_MODELS` is set. A release is not
accepted until all three exact blobs load through the current
`transcribe-cpp` path, their SHA-256 values match the host manifest, and the
release assets are anonymously reachable with resume support. The host
repository's `NOTICE.md` carries the Apache-2.0 and CC-BY-4.0 obligations for
the rehosted conversions.

The release profile keeps line tables/symbols (`debug = 1`, `strip = "debuginfo"`)
so the panic hook's backtrace can be symbolized when the matching
`src-tauri/target/release/bard.pdb` is retained with the release artifact. The hook itself writes bounded crash
reports under `%LOCALAPPDATA%\\com.miisodev.bard\\logs` before `panic = "abort"`
terminates the process; the Data pane exposes the latest report. A bounded,
secret-free `lifecycle.log` sits beside those reports and records console
interrupts, gesture outcomes, audio failure boundaries, model phases, and
OpenRouter test boundaries. It never records transcript text, credentials, or
provider response bodies.

The raw-input coordinator is the only keyboard hook. Native window operations
are queued onto the UI thread, the CPAL callback is data-only, and recording
has bounded buffers plus a 10-minute cap. The same-key `Ctrl Ctrl` gesture has
a 50 ms activation grace and a 300 ms hold threshold; its deterministic tests
are part of the normal Rust suite.

## Optional post-processing credentials

OpenRouter keys are stored through `keyring`'s Windows Credential Manager
backend. They are never written to SQLite, `AppState`, events, logs, or crash
reports. The Post-processing pane must be tested with a disposable key; the
metadata-only **Test connection** action does not send dictated text.