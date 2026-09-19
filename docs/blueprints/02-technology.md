# bard — Technology

## §0 Using This Blueprint

**Scope.** What bard runs on, how its windows relate, how transcription works,
how models are described and acquired, and how the product is built and
released. It owns the integration contracts with every outside service. It
carries no feature behaviour — that is `Product & Business` — and no visual
rules.

**Deliverables.** A buildable repository, a release pipeline, and a signed
installer.

**Path conventions.** As `Index §0`. `src-tauri/` is the Rust core, `src/` the
webview.

**Cross-reference convention.** References to other documents carry the document
name, as in `Surfaces §2`. A bare §4 is a section of this document.

**Anchor policy.** Every framework, crate, service and API here is owned by
someone else. Each is named, linked, and bounded by what bard requires of it.
Resolved versions live in `src-tauri/Cargo.toml`, `package.json` and their
lockfiles, which are the only place a version number is written.

---

## §1 The stack

bard is a [Tauri 2](https://tauri.app) application: a Rust core that owns
windows, input, storage and transcription, and a React webview that draws.

**Rust core.** Requires the MSVC toolchain. Owns everything that must be true
when no window is open — the global shortcut hooks, the clipboard monitor, the
tray, the database.

**Webview.** React with TypeScript, built by [Vite](https://vite.dev). No
router, no state library, no CSS framework. **The webview renders in WebView2**,
which ships with Windows 11.

**Why not Electron.** One Tauri codebase compiles to a native binary of roughly
eleven megabytes against Electron's hundred-plus, and the webview is the one the
operating system already runs. For a paid utility that lives in the tray,
resident memory and install size are product qualities rather than engineering
preferences.

**The webview never holds authoritative state.** SQLite is the single source of
truth. Every mutation is a command into Rust, which writes and then emits
`refresh`; the webview re-reads. *The cost of the wrong choice:* an optimistic
local cache of item state has to be reconciled with a clipboard monitor writing
from another thread, and the reconciliation is where the bugs live.

### Type safety across the boundary

`src/types.ts` mirrors the Rust structs by hand. Rust serialises `camelCase`
over a `snake_case` database, so the webview sees camelCase.

**Changing a Rust struct breaks the webview with no compile error.** The mirror
must be updated in the same change. *Phase 2* replaces the mirror with generated
bindings via [specta](https://github.com/specta-rs/specta) and
[tauri-specta](https://github.com/specta-rs/tauri-specta), which makes the
breakage a compile error instead of a runtime one.

---

## §2 Window architecture

bard owns four windows and a crowd of helpers.

| Label | Title | Size | Chrome | Purpose |
|---|---|---|---|---|
| `main` | Bard | 380 × 620 | Frameless, transparent, always-on-top, off the taskbar | The panel |
| `settings` | Bard Settings | 760 × 620 | Native | Settings |
| `draw` | Drawing | 760 × 560 | Native | The drawing canvas |
| `overlay` | Recording | 320 × 96 | Frameless, transparent, never focused | The dictation indicator |

**The panel is frameless and transparent** so its own rounded corners show the
desktop through them. **It has no window shadow.** A window frame makes the
save and restore of window size non-idempotent — the size accessors and the
setter disagree by the frame, so each launch restores what it saved plus the
frame and writes the larger number back, and the panel grows every time it
opens. A drop shadow, if one is wanted, has to be drawn by the webview into a
transparent margin inside the window, which means the application content stops
filling its own window and layout, positioning and the resize handles all
account for that. **Phase 2.**

**Settings and the canvas hide on close; they are never destroyed.** A closed
window is dropped by the framework, after which the command that opens it finds
nothing and returns silently. Their close is intercepted and hides instead.

**Secondary windows are declared in the manifest, not created at runtime**, and
each must also be listed in `src-tauri/capabilities/default.json` or every
framework call from it is denied.

### Finding bard's windows from outside

A Tauri process owns several helper windows — a single-instance window, a tray
host, a global-hotkey host, IME windows. **Windows reports the single-instance
helper as the process's main window**, and in a debug build the largest visible
window is a helper rather than the application. bard's own windows are exactly
those of window class `Tauri Window`. `scripts/shot.ps1` matches on that class;
anything else finds the wrong window.

### Window operations happen on the main thread

Calling `hide()` from a spawned thread returns success and does nothing. Any
deferred window operation dispatches back to the main thread.

---

## §3 Transcription

**One engine: [transcribe-cpp](https://crates.io/crates/transcribe-cpp).** It
loads GGUF models through ggml and covers every architecture bard offers —
whisper, parakeet, moonshine, canary, sensevoice, gigaam, qwen3 and the rest.
Each is a **single file**.

**There is no second engine.** An ONNX runtime is not required and is not
linked. *The cost of the wrong choice:* adding one drags in a prebuilt ONNX
Runtime compiled with an AVX2 baseline that executes in a static initialiser and
crashes at process start on any pre-Haswell CPU — a failure invisible on a
modern development machine and fatal on a customer's older one.

**Which GGUF, not which architecture.** A model failing to load is almost never
an unsupported family; it is a conversion in a layout the engine does not read.
Third-party conversions of the same model are not interchangeable.

**Acceleration is Phase 2.** Non-default crate features reach a Vulkan GPU
backend and per-ISA CPU modules scored at runtime, which is the answer to
transcription running at one to four times realtime on CPU. Those features make
the native layer build ggml from source, which is a build-environment problem
before it is a code one — see §7.

---

## §4 The model catalogue

`src-tauri/src/catalog/catalog.json`, compiled into the binary.

**It is baked in rather than fetched.** The list a new user first sees has to be
there on a machine that has never been online, and a catalogue that can fail to
load can leave the application with nothing to offer. Updating it means shipping
a build, which is the right cadence for entries carrying checksums.

### Entry contract

Every entry carries the same fields.

| Field | Meaning |
|---|---|
| `id` | Stable identifier; the key the selected model is stored under |
| `name`, `description` | What the settings surface shows |
| `architecture` | **Which engine path loads it.** Nothing in a model's own files states this reliably |
| `family`, `parameters` | Lineage and size, for the reader |
| `languages` | Count; 1 renders as "English only" |
| `capabilities` | `streaming`, `translate`, `languageDetect` |
| `accuracy`, `speed` | 0–100, coarse, **relative to the other entries and to no benchmark** |
| `recommended`, `recommendedRank` | Ordering for the curated list |
| `file` | `filename`, `url`, `sha256`, `sizeBytes`, optional `quant` |
| `source` | Upstream `repo` and `revision` |

**`architecture` is the load-bearing field.** Adding a model is adding a row
here, never a branch in the loader.

**Scores are curation.** A model installed from Hugging Face has none, and the
settings surface shows no bars for it — empty bars would claim it had been
measured and found wanting.

### Where model files come from

**Anchored, pinned and verified.** Each entry names an upstream repository and a
commit, and carries the file's own SHA-256. Whisper models are
[ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp) — the
reference ggml builds, which is their canonical home.

**bard requires of the host:** ranged requests, so an interrupted download
resumes; a stable URL for a pinned revision; and public access without
authentication.

Download verifies the checksum before the file is accepted. A mismatch fails
loudly rather than installing a file that will fail at first use.

**Architectures whose GGUF conversions upstream does not publish are converted
and hosted under bard's own organisation.** **Phase 2**, and it gates how wide
the catalogue can get rather than whether bard ships.

---

## §5 Hugging Face integration

Three ways to get a model, in the order a person tries them: the curated
catalogue, models already in the machine's shared cache, and any repository by
name.

**Cache discovery.** The shared cache stores snapshots as links into a blob
store. **Directory-entry metadata does not follow links**, so reading it
measures the link rather than the model and every model reports as empty.
Resolving metadata through the path is what reports a real size.

**Repository listing.** The tree API at
[huggingface.co/docs/hub/api](https://huggingface.co/docs/hub/api) supplies each
file's path, size and LFS object id. **The LFS object id is the file's SHA-256**,
which is what makes an arbitrary repository installable with the same
verification the catalogue gets.

**bard requires of the API:** anonymous read of public repository trees, LFS
object ids in the listing, and redirect-following resolve URLs that honour
range requests.

**Installation is load-verified.** A downloaded model is registered only after
the engine has actually opened it.

---

## §6 Storage layout

```
%APPDATA%\com.miisodev.bard\
  bard.db          the single source of truth
  images\          captured and drawn images, by timestamp
  models\          downloaded model files
  clean-exit       marker; see §9
```

The identifier is fixed by the application manifest and the database path is
derived from it directly, because the database opens before the framework is
available to resolve it.

---

## §7 Build

```sh
bun install
bun run tauri dev      # dev server and app
bun run tauri build    # MSI and NSIS installers
bun run build          # typecheck, CSS audit, bundle — the frontend gate
```

`bun run build` is the gate: TypeScript strict with unused-symbol checks, then
`scripts/css-audit.mjs`, then the bundle. **The CSS audit fails the build on
duplicate selectors and undefined custom properties**, neither of which a
typecheck can see and both of which are silent at runtime — a duplicated
selector is not an error, it is a rule that quietly wins.

### Prerequisites

| Requirement | Why |
|---|---|
| Rust, MSVC toolchain | The core |
| [Bun](https://bun.sh) or Node 18+ | The webview build |
| Visual Studio Build Tools | MSVC, for native crates |
| **CMake** | The transcription native layer builds from source under non-default features |
| WebView2 | Preinstalled on Windows 11 |

**The traps are recorded in `docs/BUILD.md`**, and each fails with a message
naming something other than its cause: CMake present but absent from PATH; the
native layer's path-shortening symlink under `%LOCALAPPDATA%`; the dev server
dying with the shell that launched it; the guardian resurrecting a killed
process unless `BARD_NO_GUARDIAN` is set; and in-place editing being required
because a rename-based write defeats the bundler's file watcher.

---

## §8 Configuration and secrets

bard has no runtime configuration and no secrets. It contacts no service on its
own behalf, holds no API key, and reads no environment variable in normal
operation.

| Name | Reaches | Purpose |
|---|---|---|
| `BARD_NO_GUARDIAN` | The application, development only | Suppresses the relaunch guardian |
| `--autostart` | The application, from the login entry | Marks a launch as started by Windows |

**Signing credentials are the one secret**, held by the release operator and
never in the repository.

---

## §9 Process lifecycle

**Single instance.** A second launch signals the first, which shows its panel,
and exits.

**Autostart re-asserts its own path on launch**, so an install to a new location
does not leave Windows launching the old one. **Debug builds never write the
login entry.** Autostart registers whichever executable is running, so one
development session otherwise repoints what the machine launches at login at a
build that only works while a dev server happens to be up.

**The guardian is a workaround and is named as one.** It watches for the process
dying within twenty seconds of launch without leaving a clean-exit marker, and
relaunches once. It exists because of a WebView2 startup flake on transparent
frameless windows. **It has a cost that was not chosen:** a process that
relaunches itself also survives being ended from Task Manager, which is
user-hostile. **Phase 2 removes it** by finding the startup failure, and
whatever replaces it must not resurrect a deliberately killed process.

---

## §10 Verification tooling

An always-on-top DWM-composited panel cannot be captured by ordinary desktop
capture, so the repository carries its own.

| Script | Purpose |
|---|---|
| `scripts/shot.ps1` | Captures a window's pixels directly; `-Show`, `-Screen`, `-Title` |
| `scripts/drive.ps1` | Clicks, types and drags at window-relative coordinates |
| `scripts/stop.ps1` | Stops the application the way the guardian considers clean |
| `scripts/css-audit.mjs` | Duplicate selectors, undefined properties, unused classes |

**These verify the installed application, not a development build.** A
development build cannot be run by the person who bought the product, so
evidence from it is evidence about something else. Two consequences follow, both
learned expensively: a window raised with `ShowWindow` from outside is not one
the framework believes is visible, so a later hide does nothing and auto-hide
looks broken when it is not; and a capture of a hidden window returns its last
painted frame rather than failing, so it silently shows the past.

---

*Technology · version 1.0*
