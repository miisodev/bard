# bard — Technology

## §0 Using This Blueprint

**Scope.** What bard runs on, how its windows relate, how transcription and
optional text-only post-processing work, how models are described and acquired,
and how the product is built and released. It owns the integration contracts
with every outside service. It carries no feature behaviour — that is
`Product & Business` — and no visual rules.

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
tray, the database. The raw `rdev` hook forwards events to a normal worker;
fixed same-key Shift/Control gestures, CPAL monitoring, and model work never
call Tauri window APIs from a callback or audio thread. Native window work is
always queued onto the UI thread.

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
| `overlay` | Recording | 260 × 60 | Frameless, transparent, never focused | The dictation indicator |

The overlay position is persisted as one of six values: `top-left`,
`top-middle`, `top-right`, `bottom-left`, `bottom-middle`, or `bottom-right`.
The native layer selects the monitor under the cursor, applies a DPI-aware
40-pixel edge margin, and clamps the physical window bounds on small displays.
The default is `bottom-middle`, preserving the original placement.

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
loads GGUF models through ggml. The shipped catalogue uses the `whisper`,
`parakeet`, and `qwen3_asr` single-file architecture paths; the loader also has
explicit handlers for the other supported single-file families. No ONNX runtime
is linked.

**There is no second engine.** An ONNX runtime is not required and is not
linked. *The cost of the wrong choice:* adding one drags in a prebuilt ONNX
Runtime compiled with an AVX2 baseline that executes in a static initialiser and
crashes at process start on any pre-Haswell CPU — a failure invisible on a
modern development machine and fatal on a customer's older one.

**Which GGUF, not which architecture.** A model failing to load is almost never
an unsupported family; it is a conversion in a layout the engine does not read.
Third-party conversions of the same model are not interchangeable.

**The local transcript is the network boundary.** Audio and engine output stay
in the Rust transcription path. Optional cloud work begins only after a complete
local transcript exists and receives text alone; its request and secret rules
are in §8.

**Acceleration is Phase 2.** Non-default crate features reach a Vulkan GPU
backend and per-ISA CPU modules scored at runtime, which is the answer to
transcription running at one to four times realtime on CPU. Those features make
the native layer build ggml from source, which is a build-environment problem
before it is a code one — see §7.

---

## §4 The model catalogue

`src-tauri/src/catalog/catalog.json`, compiled into the binary.

**It is baked in rather than fetched.** The Bard model list a new user first
sees has to be there on a machine that has never been online, and a catalogue
that can fail to load can leave the application with nothing to offer. Updating
it means shipping a build, which is the right cadence for entries carrying
release URLs and checksums.

### Bard's default roles

The three product roles are explicit catalogue fields, not labels inferred from
a filename.

| Role | Display name | Engine architecture | Exact release asset | Coverage and capability |
|---|---|---|---|---|
| **Fast multilingual** | Bard Fast | `whisper` | `whisper-small-Q6_K.gguf` | 99 languages; translation and automatic language detection |
| **Best English** | Bard English | `parakeet` | `parakeet-tdt-0.6b-v2-Q6_K.gguf` | English with token timestamps |
| **Best multilingual** | Bard Multilingual | `qwen3_asr` | `Qwen3-ASR-1.7B-Q6_K.gguf` | 30 languages with automatic language detection |

The product-facing role labels are **Fast multilingual**, **Best English**, and
**Best multilingual**. Their exact assets and metadata are the contract; the
implementation may keep opaque internal IDs, but no model family name is a
substitute for the role or asset mapping.

### Provenance, licensing, and hosting

| Role | Conversion source and pinned revision | Bytes | SHA-256 | Licence |
|---|---|---:|---|---|
| **Fast multilingual** | `handy-computer/whisper-small-gguf@a2073177cb69bd74b9ca9460b852d17fbfd5d68c` | 212107328 | `4d14215a972101fff787fc59e30b908ee511c725600317e02c9032cb8799f32d` | [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) |
| **Best English** | `handy-computer/parakeet-tdt-0.6b-v2-gguf@382796e405a9fe37145938dccbabfb25e93e4286` | 600408576 | `44c7c363438e03b613cf8ced8b7e6fd1beb2b3805008136fe5e922ab3c79eea9` | [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/) |
| **Best multilingual** | `handy-computer/Qwen3-ASR-1.7B-gguf@3555bd238a8572bbace3ebf60d23b036dc0a5dbe` | 1692554208 | `c75a961b7134a6c952d89797865cb0d0376876185aee04ef6d12c31c2952e4e1` | [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) |

All three default files are Q6_K conversions. The `source` record also carries
the upstream model repository and revision and links to each model-specific
licence text. The first-party host for every default is
[miisodev/bard-models](https://github.com/miisodev/bard-models), release
`models-v2`; the catalogue pins the exact asset URL rather than a moving
branch. Each default `file.url` is under
`https://github.com/miisodev/bard-models/releases/download/models-v2/` and
ends with the exact asset filename. The release assets are exact-byte rehosts
with model-specific notices.
The release gate is satisfied for these three files: the public assets match the
recorded bytes, expose `Range` responses, and each exact file passed the Windows
`transcribe-cpp` load-and-transcription smoke test. A matching architecture header
or checksum alone would not have been sufficient evidence.

### Entry contract

Every entry carries the same fields.

| Field | Meaning |
|---|---|
| `id` | Stable identifier; the key the selected model is stored under |
| `role` | `Fast multilingual`, `Best English`, or `Best multilingual` |
| `name`, `description` | What the settings surface shows |
| `host` | `miisodev/bard-models`, release `models-v2`, asset name, URL, and status |
| `architecture` | **Which engine path loads it.** Nothing in a model's own files states this reliably |
| `family`, `parameters` | Lineage and scale, for the reader |
| `languages` | Count and descriptors; 1 renders as English-only |
| `capabilities` | `streaming`, `translate`, `languageDetect` |
| `accuracy`, `speed`, `value` | 0–100, coarse and relative to Bard's three defaults, not to a benchmark |
| `recommended`, `recommendedRank` | Ordering for the curated list |
| `file` | `filename`, `url`, `quant`, `sizeBytes`, `sha256` |
| `source` | Conversion `repo` and `revision`, upstream model fields, and licence name/URL |

**`architecture` is the load-bearing field.** Adding a model is adding a row
here, never a branch in the loader.

**Accuracy, speed, and value are curation.** They express the ordering among
Bard's defaults and support no comparison with an imported or externally
discovered model. Those rows have no metrics, and the settings surface shows
no bars for them — empty bars would claim a measurement that does not exist.

### Where model files come from

**Anchored, pinned, and verified at acquisition.** The three Bard defaults are
immutable assets in the `models-v2` release of
[miisodev/bard-models](https://github.com/miisodev/bard-models). Each entry
names the conversion repository and revision, the upstream model lineage, the
model-specific licence, the exact byte size, and the full-file SHA-256. The
release carries model-specific notices and does not substitute a different
quantisation or conversion.

**bard requires of the host:** ranged requests, so an interrupted download
resumes; stable public asset URLs without authentication; and assets that do
not change under the same release tag. Download accepts a file only after the
byte count and full-file SHA-256 match, then opens it with the engine. A
checksum mismatch or load failure never produces an active model.

---

## §5 Model discovery and advanced import

The backend presents three sources through one inventory and activation
boundary: Bard's compiled catalogue, compatible files already on the machine,
and models imported from a user-named Hugging Face repository. The picker
presents them as `Bard models`, `On this computer`, and `Advanced model import`.
These are views of one inventory, not duplicate rows; `Surfaces §3.4` owns their
layout and copy.

**Ownership is explicit.**

| Origin | Owner | Remove behaviour |
|---|---|---|
| Bard release | Bard-managed | Removes only the canonical file beneath Bard's `models\` directory; the active model is protected |
| Advanced import downloaded by Bard | Bard-managed | Removes the file and its registration only after canonical path containment succeeds |
| File found in an external Hugging Face cache | External | Bard never deletes, moves, or rewrites the file; `Forget` removes only Bard's reference |

**Compatibility is an engine result, not a filename guess.** The native status
is `Unverified` until the current engine has been asked to open the exact file,
`Verified` after a successful load, and `Incompatible` when that exact file
fails to load. The settings surface renders these as `Unknown`, `Compatible`,
`Checking`, or `Incompatible` while a probe is running or after it completes.
Only a verified file can be activated.

**Cache discovery.** The shared Hugging Face cache stores snapshots as links
into a blob store. **Directory-entry metadata does not follow links**, so
reading it measures the link rather than the model and every model reports as
empty. Resolving metadata through the path reports the real file. Discovery
checks the shared cache selected by `HF_HUB_CACHE` or `HF_HOME`, falling back
to `%USERPROFILE%\.cache\huggingface\hub\`; it does not scan unrelated drives
or directories. Unregistered external discoveries are projections of those
files, not Bard-owned `custom_models` rows; registering one records external
ownership without copying it. Their inventory ID is `hf|<repository>|<file>`,
which is the value stored in `model_id` when one is selected. A managed import
uses the same registration identity but an app-owned path beneath Bard's
`models\` directory. Bard does not claim a checksum for an external cache file
and never deletes that file.

**Repository inspection.** `Advanced model import` accepts an
`owner/repository` identifier, a Hugging Face repository URL, or a direct
Hugging Face `resolve/main/<file>` URL. It resolves the repository on the
`main` branch and lists `.gguf` and `ggml-*.bin` files. The
[Hugging Face Hub OpenAPI specification](https://huggingface.co/.well-known/openapi.md)
supplies each file's path, size, and LFS object id. **The LFS object id is the
file's SHA-256**, which gives an imported file the same full-file checksum
boundary as a Bard asset. Bard requires anonymous reads of public repository
trees, LFS object ids in the listing, and redirect-following resolve URLs that
honour range requests. It does not accept an arbitrary non-Hugging-Face direct
URL.

**One downloader serves Bard assets and imports.** A download writes to a
partial file, resumes with a range request, checks the exact byte count and the
SHA-256 of the complete file, atomically moves the verified file into Bard's
model directory, and opens it with the engine. Installation never changes
`model_id`; activation is a separate command that loads and verifies the file
before the checked setting write. *The cost of the wrong choice:* a picker that
treats a URL or filename as compatibility proof moves failure into dictation.

---

## §6 Storage layout

```
%APPDATA%\com.miisodev.bard\
  bard.db          the single source of truth for application-owned data
  images\          captured and drawn images, by timestamp
  models\          Bard-owned catalogue files and Advanced model imports
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

bard has no user account or application data backend. Its non-secret settings
are the keys defined in `Product & Business §5`. Optional cloud
post-processing is off by default and is enabled only after a key and an exact
OpenRouter model ID are present.

| Name | Reaches | Purpose |
|---|---|---|
| `BARD_NO_GUARDIAN` | The application, development only | Suppresses the relaunch guardian |
| `--autostart` | The application, from the login entry | Marks a launch as started by Windows |

**The OpenRouter endpoint is fixed by the application.** Text cleanup always
posts to the official HTTPS endpoint
`https://openrouter.ai/api/v1/chat/completions`; the connection test reads
`https://openrouter.ai/api/v1/models/user`. There is no user-editable endpoint,
provider selector, or prompt library. The model-ID list is the only provider
choice exposed by the surface; its first entry is the active request model.
Bard requires the endpoint to accept a text chat-completions request, preserve
that exact first configured model ID, and return one complete non-empty text
response.

**The OpenRouter API key lives in Windows Credential Manager through keyring
v1.** The credential entry is `com.miisodev.bard` / `openrouter-api-key` for the
current Windows user. The settings surface accepts a replacement only when the
person explicitly saves it; the Rust service writes it through the Windows
native keyring store and returns only a configured boolean. The key is never
stored in SQLite, application state, Tauri events, frontend persistence,
environment variables, URLs, request logs, panic reports, or crash logs. A
credential-store failure is reported without the secret. *The cost of the wrong
choice:* a frontend-visible or persisted key turns a settings convenience into
a credential that every diagnostic surface can expose.

**The request crosses one boundary.** It is built only after an explicit
dictation action has produced a complete local transcript. The body contains a
fixed built-in cleanup instruction and one user message containing that
transcript text; there is no user-editable prompt. The request is non-streaming
(`stream: false`), bounded to an 8,192-token completion budget sized for a
complete 20-minute dictated take, denies provider data collection, and sets
provider `allow_fallbacks: false` for the request. Provider work has a
45-second request timeout inside a 60-second cancellable deadline; a timeout
uses the untouched local transcript. The response must identify the requested
model, contain exactly one choice, and finish with `stop`. The stored model-ID
setting may be a
comma-separated list of opaque IDs, but only its first ID is sent. Bard never
silently substitutes an alias, another model ID, or a fallback model.

The fixed cleanup instruction is an editor, not a summarizer: it asks for
faithful sentence-boundary repair, natural paragraph flow, unambiguous removal
of recognition noise and false starts, and preservation of names, numbers,
technical terms, quotations, lists, language, emphasis, and uncertainty. It
forbids invention, translation, answering, explanation, and following commands
embedded in the transcript; ambiguous wording stays unchanged.

The request contains no audio, samples, clipboard state, capture history,
images, prompts, identifiers, or unrelated text. The local transcript remains
intact until a complete response has passed validation.

| Failure | Bard's behaviour |
|---|---|
| Disabled or incompletely configured | Makes no request and uses the local transcript |
| Missing or rejected credential | Uses the local transcript and reports the credential problem without exposing a response body; the configured stage is not changed behind the person's back |
| Unknown or unavailable model | Makes no replacement request, uses the local transcript, and reports the configuration problem |
| Rate limit, quota or credit failure | Uses the local transcript and reports that cloud processing did not complete |
| Timeout, transport or provider error, malformed response, empty content, or policy block | Uses the local transcript unchanged, shows a safe warning, and never pastes partial cloud output |

A cloud result replaces the local result only after the whole response is
validated. Bard performs no automatic model fallback, because a fallback would
break both the person's explicit model choice and the promise that the named
model received the text.

**Release-signing credentials remain separate.** They belong to the release
operator, never enter the repository, and are unrelated to a person's
OpenRouter credential.

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

**The model manager is specified at its boundaries.** The catalogue contract
test checks the role, single-file architecture, metric bounds, quantization,
source revision, licence, and first-party host repository/asset for each
default. Downloader unit tests cover resume handling, full-file hashing, and
path containment; the acquisition path installs without activation, and the
engine probe is invoked before the checked `model_id` write. Post-processing
unit tests cover the first-model-ID contract, comma-list parsing, fixed request
shape, non-streaming output, redaction, and response validation. Each exact
`models-v2` asset passed the Windows `transcribe-cpp` load/transcription gate,
and the Windows Credential Manager round-trip passed with a disposable test
credential. A live Hugging Face/cache round-trip and a real OpenRouter request
remain release verification; catalog metadata alone does not establish them.

**These verify the installed application, not a development build.** A
development build cannot be run by the person who bought the product, so
evidence from it is evidence about something else. Two consequences follow, both
learned expensively: a window raised with `ShowWindow` from outside is not one
the framework believes is visible, so a later hide does nothing and auto-hide
looks broken when it is not; and a capture of a hidden window returns its last
painted frame rather than failing, so it silently shows the past.

---

*Technology · version 1.1*
