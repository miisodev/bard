# bard — parity ledger

The build plan. bard ships when every row below is **done** and the whole thing
survives the two tests:

> **The Copper test.** A long-time heavy user of shadcn's Copper picks up bard and
> uses it with no caveats and no friction.
>
> **The Handy test.** A long-time heavy user of Handy picks up bard and uses it
> with no caveats and no friction.

And the standard above both: *would Apple ship this, and would they show it at WWDC.*

## What bard is

A paid ($30, one-time) native Windows app: Copper's capture/clipboard workspace and
best-in-class offline dictation in one keyboard-first panel. Sold on miiso.dev,
possibly the Microsoft Store later.

Dictation targets Wispr Flow's *capability* (minus notetaking) but delivers it
**fully offline on open weights**, with genuine freedom of model choice — paste a
Hugging Face repo link, or pick up a model already on the machine — without the
configuration sprawl that would lose a non-technical user. Handy's curated
recommendation list is the pattern to follow.

## Reference material

| Source | What it is | Where |
| --- | --- | --- |
| **shadcn's Copper** | **The target.** Mac app, closed source | `~/Downloads/Cooper/*.mp4` (3 demos), shadcn.com/copper |
| `cooper/` | TouchMyBar/cooper — Apache-2.0 open-source Copper recreation, v0.3.75. Code reference, *not* the target | fork of `TouchMyBar/cooper` |
| `Handy/` | Handy — dictation app, forked for reference. Also installed on this machine | `OneDrive/Desktop/Handy/handy.exe` |
| `bard/` | us | `miisodev/bard` |

**cooper is a means, Copper is the end.** Where cooper deviates from Copper, Copper wins.

## Where we actually stand

Measured 2026-09-19, first session with vision on the project.

| | bard | cooper | Handy |
| --- | --- | --- | --- |
| Source LOC | 6,274 | 6,393 | 43,952 |
| Tauri commands | 52 | 40 | 53 |
| User-facing settings | **9** | ~15 | **62** |

bard is roughly *one* of its two references in size while owing the surface of both.
The v0.1.0 build is a working skeleton with both halves sketched — not a parity build.

---

## A. Copper parity — the capture/clipboard half

Verified against the Copper demo videos and cooper's source.

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| A1 | **Done-state checkboxes** — hollow circle to filled check + strikethrough | **done** | `done` column, `set_done`/`clear_completed`, Space toggles the selection off its lead item. Favorites survive behind the overflow menu as a filter |
| A2 | **Inline markdown** — bold, italic, strikethrough, code; underscore emphasis deliberately unsupported | **done** | `src/markdown.tsx`; composes with search highlighting so a hit inside emphasis still marks. All four forms verified on screen |
| A3 | **Glass theme** — DWM acrylic, alongside System/Light/Dark | **done** | Acrylic was previously applied *always*, at an alpha opaque enough to read as a slab, under a web layer that painted over it anyway. Now it is the Glass theme only, cleared otherwise, with `data-glass` switching surfaces translucent. Verified composited: the desktop reads through the panel and the cards |
| A4 | **Light theme quality** | **done** | Rebuilt off the measured Copper values. Panel `#e6e7ea` vs Copper `#e5e6e9`, card `#f9f9fb` vs `#f8f8f8`, field `#f3f4f7` exact |
| A5 | **Sub-branches / nesting** — `add_child`, branch style, per-item collapse | **missing** | cooper: `BranchPanel.tsx`, nesting to 10 deep |
| A6 | **Section collapse** — `##` clears the board, others compress to one-line dropdowns | **missing** | `set_section_collapsed` absent |
| A7 | **Drawing** — composer plus-menu to "Create Drawing…" to canvas window to an item in the panel | **done** | Own window (`draw.html`), native Windows chrome because it is a document window rather than a summoned overlay. Select / pen / line / arrow / rect / ellipse / text, five colours, three widths, undo, redo, delete, Save and Copy and Save. `src/draw/shapes.ts` is a fresh pure model — no React, no Tauri — rather than a copy of cooper's, which is Apache-2.0 and would have entangled a paid product. Whole round trip verified on screen |
| A8 | **Composer plus-menu** — "Attach Files…" / "Create Drawing…" | **done** | The plus sits *inside* the field, as in Copper — beside it, it stole enough width to clip the placeholder. Shared `Menu` component with the header overflow |
| A9 | **Click thumbnail to copy image to clipboard** | **done** | The row was wrong: `copy_item` already handled images, only the click affordance was missing. Card images and attachment thumbnails now copy on click |
| A10 | **Copy as List** — multi-select to Markdown list (`Ctrl+Shift+C`) | **verify** | Multi-select exists; list-copy path unconfirmed |
| A11 | **Shortcuts sheet** (`Ctrl+/`) and shortcuts editor | **missing** | `set_keybinds` exists; no UI. cooper has both |
| A12 | **Focus hand-back** — Esc returns focus to the previous app | **verify** | `panel.rs` remembers and restores the previous foreground window; still untested by hand |
| A13 | **Right-Shift x2 toggles panel** (Left-Shift x2 captures) | **verify** | bard binds double-shift; side-specific behaviour unconfirmed |
| A14 | **Obsidian export** — vault detection, one file per section | **missing** | cooper has all three commands |
| A15 | **Merge notes** | **missing** | Named on Copper's own feature list |
| A16 | **Update checker** + in-app update banner | **missing** | cooper: `update.ts`; Handy: full `update-checker/` |
| A17 | **Sound preferences** — per-event sounds, themes, volume | **partial** | bard has one `sound_on_capture` boolean; cooper has prefs, Handy a picker + volume |
| A24 | **Dismiss on losing focus** — the panel goes away when you move on | **done** | Driven by the native window's focus event, not a webview blur listener: the webview is not reliably told when the *native* window loses activation, which is the case that matters. A 140 ms grace absorbs focus flicker, and the panel stays put when the foreground window belongs to bard, so opening settings or the canvas does not yank it away |
| A18 | **Empty state** — contextual, uses the active section name | **done** | Asks "What should we build in &lt;section&gt;?" over a quiet glyph, with a first-run hint about double-shift. Verified, then centred and the hint re-cut to stop it orphaning a word |
| A19 | **No window chrome** — frameless, keyboard-dismissed | **done** | Titlebar row removed |
| A20 | **No branding in the panel** | **done** | Wordmark removed; the header is a search pill and one overflow button |
| A21 | **No timestamps on cards** | **done** | The whole per-card action row went with it — actions are keyboard and context menu, as in Copper |
| A22 | **Section header style** — small-caps letterspaced label + hairline rule | **done** | |
| A23 | **Composer copy** — "Add a note, type a prompt, or describe a task" | **done** | `#`/`##` still work; the placeholder no longer teaches syntax |

## B. Handy parity — the dictation half

bard exposes 9 settings; Handy exposes 62. Every row is a user-visible capability.

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| B1 | **Curated model catalog** — recommended ranking, accuracy/speed bars, language count, streaming/translate badges, size, checksums | **done** | `src-tauri/src/catalog/catalog.json`, baked in at compile time so a machine that has never been online still has a list to offer. Carries `architecture`, which is what decides the engine — bard's own curation, not a copy of Handy's file. Not yet: multiple quants per model, and the catalog is still whisper-only because only whisper can be acquired (see B5) |
| B2 | **Model manager UI** — downloaded vs available, search, language filter, per-model Active/Delete, progress with speed + cancel | **partial** | Now a real pane, in three groups in the order a person tries them: the curated list, models already in the machine's HF cache, then any repo by name. Still missing search, language filter, delete, and download speed |
| B3 | **HF repo paste + one-click install** | **done** | Resume, checksum verify, cancel, load-verified registration |
| B4 | **Discover models already on the machine** | **done** | HF cache scan with one-click Use |
| B5 | **Non-whisper architectures** (parakeet, nemotron, moonshine, canary, sensevoice, gigaam, cohere) | **in progress** | `transcribe-rs` (`onnx`) is now a dependency alongside `transcribe-cpp`. The remaining work is not the crate but the **shape of a model**: `ParakeetModel::load(dir, quantization)` takes a *directory* of ONNX files, where bard treats a model as a single file path everywhere — acquisition, storage, the loader. And nothing in those files reliably states which family they are, so the loader must be told. **B5 therefore depends on B1**, which is where a model's architecture would be recorded |
| B6 | **Transcription history** + retention period + limit | **missing** | Handy: `managers/history.rs`, history settings pane |
| B7 | **Post-processing via LLM** — provider, base URL, API key, model, prompt library | **missing** | Handy: `llm_client.rs` + post-processing settings. bard removed its Ask layer |
| B8 | **VAD** — enable, backend selector (earshot/silero), smoothing | **partial** | `dictation/vad.rs` exists; no settings |
| B9 | **Paste method / clipboard handling / paste delay / reliable paste** | **partial** | `paste.rs` is 47 LOC; Handy has per-OS `paste_tx/` + 5 settings |
| B10 | **Auto-submit** (+ key) | **missing** | |
| B11 | **Append trailing space** | **missing** | |
| B12 | **Custom words / vocabulary** + correction threshold | **missing** | |
| B13 | **Filler-word removal** + custom filler list | **partial** | Filler cleanup exists; not configurable |
| B14 | **Translate to English** / language selector | **missing** | |
| B15 | **Mute while recording** / output device / volume | **missing** | |
| B16 | **Always-on microphone**, channel selector | **missing** | |
| B17 | **Model unload timeout** | **partial** | Fixed 5-minute idle unload; not configurable |
| B18 | **Hardware acceleration** — GPU backend + runtime CPU-ISA dispatch, and a selector | **blocked on the build** | The change itself is one line: on Windows x86_64, `transcribe-cpp` with `features = ["dynamic-backends", "vulkan"]` — a Vulkan GPU backend plus per-ISA CPU modules scored at runtime. But any non-default feature set makes `transcribe-cpp-sys` compile ggml from source, and that build does not come up on this machine: it shortens paths through a symlink under `%LOCALAPPDATA%	cs` pointing at a per-build `out/` directory, and CMake cannot write into it (`os error 267`, then `Unable to open check cache file for write`). CMake also is not on PATH — see `docs/BUILD.md`. Solve the from-source build first; it is the gate for the whole acceleration story. (ONNX Runtime stays CPU-only on Windows on purpose — DirectML pulled a prebuilt with an AVX2 static initialiser that crashes pre-Haswell CPUs) |
| B19 | **Recording overlay** — position, style, on/off | **partial** | Overlay exists; unverified by eye, no settings |
| B20 | **Shortcut activation mode** (hold vs toggle) + hold threshold | **missing** | |
| B21 | **Audio feedback** — start/stop sounds, theme, volume | **partial** | Capture sound only |
| B22 | **Start hidden / show tray icon / autostart** | **partial** | Autostart done and now in General; start-hidden and tray-icon toggles still missing |
| B23 | **Debug pane** — log level, live log viewer, paths, keyboard diagnostic | **missing** | |
| B24 | **What's new on update** | **missing** | |
| B25 | **i18n** — app language | **missing** | Handy ships a full i18n layer. *Scope call needed* |

## C. Cohesion — where bard earns its own existence

Neither reference does this; it is the reason bard is one app and not two.

| # | Item | Status |
| --- | --- | --- |
| C1 | Dictation lands in the panel when open, pastes into the focused app when closed | **verify** |
| C2 | Dictate directly into the composer (voice to note) | **missing** |
| C3 | Dictate into an existing card (voice to edit / append) | **missing** |
| C4 | One coherent shortcut map across both halves, no collisions | **missing** |
| C5 | One settings surface where dictation and clipboard read as one product | **missing** |
| C6 | One onboarding covering both halves | **partial** |

## D. Apple-standard polish

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| D1 | **Design system** — tokens, type scale, spacing, motion, both themes | **done** | `src/styles/tokens.css`: type, space, shape, motion, four themes on two axes, reduced-motion. A legacy-alias block at the end shrinks to nothing as components migrate |
| D2 | **Motion** — panel in/out, card insert, state changes; measured, never bouncy | **partial** | Tokens and per-control transitions are in. The panel's own show/hide and list reordering still need a pass |
| D3 | **Startup** — cold boot time, no flash, no crash | **partial** | Guardian self-heals a ~1-in-15 WebView2 startup crash. **A guardian is a patch, not a fix — find the root cause.** It also has a side effect nobody chose: because it relaunches any death inside 20s that left no clean-exit marker, **End Task in Task Manager resurrects the app.** An app that will not stay killed is user-hostile; whatever replaces the guardian must not do this |
| D4 | **Dictation engine** | **resolved** | Two engines, as Handy does it: `transcribe-cpp` (+ `dynamic-backends`, `vulkan` on Windows x86_64) for whisper-family GGUF, `transcribe-rs` (`onnx`) for Parakeet/Moonshine/Canary/SenseVoice/GigaAM/Cohere. No rebuild needed. Handy is **MIT**, so its approach and code are ours to use with attribution |
| D10 | **Settings has its own window** | **done** | 760x620, native chrome, sidebar: General / Capture / Dictation / Models / Shortcuts / Data / About. `src/settings/ui.tsx` carries the primitives — Pane, Group, Row, Toggle, Segmented, Select, Button, TextField, Note, Progress — so a new setting is a line of markup rather than a design decision taken again. The old sheet is deleted |
| D9 | **Generate `types.ts` instead of hand-mirroring it** | **missing** | Today a Rust struct change breaks the frontend with no compile error. Handy generates bindings with `specta` + `tauri-specta`. Worth adopting before the surface triples |
| D5 | **Real-audio verification** — the mic has never been exercised | **missing** | |
| D6 | **Visual QA of every surface** | **in progress** | Verified: panel (light, dark, glass), overflow menu, empty state, composer menu, the drawing window and its round trip, and the settings window's General, Models and Shortcuts panes. Still unseen: onboarding, the dictation overlay, the section switcher, the context menu, and the Capture / Dictation / Data / About panes |
| D7 | **Accessibility** — keyboard reachability, focus rings, contrast, screen reader | **missing** | |
| D8 | **Empty / error / loading states everywhere** | **missing** | |

## E. Ship

| # | Item | Status |
| --- | --- | --- |
| E0 | **ONNX Runtime must not be pyke's prebuilt** | **blocking** | Adding `transcribe-rs` pulled `ort`, whose default features download pyke's prebuilt ONNX Runtime — the exact binary Handy's manifest refuses, because it is compiled with a global `/arch:AVX2` baseline that executes BMI2/AVX **in a static initialiser** and so crashes at process start on any pre-Haswell CPU (Sandy/Ivy Bridge, older AMD). It runs here and would crash on a customer's older machine, silently and at launch. bard cannot switch this off from its own manifest: `transcribe-rs` declares `ort` with default features, and Cargo features are additive. The fix is Handy's — supply a baseline ONNX Runtime and point `ORT_LIB_LOCATION` at it at build time. **Until that is done, no installer built from this tree goes to anyone** |
| E8 | **Stop downloading models from Handy's CDN** | **blocking** | All three curated models are fetched from `blob.handy.computer`. That is Handy's own bandwidth, and bard is a product we charge for — serving our customers' downloads off another project's infrastructure is not something to ship, regardless of licence. Replacing the URLs needs verified checksums for the new host, so it is a deliberate task rather than a find-and-replace |
| E1 | Licensing — $30 one-time key issue + offline validation | **missing** |
| E2 | Code signing (SmartScreen) | **missing** |
| E3 | Auto-update channel + signed releases | **missing** |
| E4 | miiso.dev purchase + download page | **missing** |
| E5 | Microsoft Store packaging | **later** |
| E6 | Licence review — cooper is Apache-2.0, Handy is MIT | **clear, keep it that way** | bard ships **no code from either**. Handy was read for facts — which crates, which features, what its catalog records — and facts carry no licence. cooper was read for approach only; `shapes.ts` and `markdown.tsx` are bard's own. The markdown tokenizer *was* a verbatim copy of cooper's regex and has been rewritten as an independent scanner. Attribution attaches to code, so none is owed today — but it becomes owed the moment anything is vendored, `catalog.json` included |
| E7 | Name/brand check against shadcn's Copper — bard must not read as a clone or a knock-off | **open** |

---

## Status vocabulary

**done** verified working · **partial** exists, incomplete · **missing** not built ·
**verify** may exist, unconfirmed · **off** built but wrong against the target ·
**regression** built in a way that actively breaks a reference behaviour ·
**blocked** needs a decision or an upstream fix · **open** decision not yet made

## Working rules

1. **Nothing is done until it has been looked at.** Capture the window
   (`scripts/shot.ps1`) and inspect it. v0.1.0 shipped unseen; that does not happen again.
2. **Copper over cooper** wherever they disagree.
3. **No feature creep.** Every row here traces to Copper, Handy, or cohesion.
4. **A workaround is logged as a workaround**, never as a fix (see D3).

---

## Log

**2026-09-19 — design system and the Copper shell.** Ledger opened. Built
`tokens.css` and rebuilt the panel against it: done-state checkboxes, inline
markdown, no window chrome, no wordmark, no timestamps, Copper's section headers
and composer copy, an overflow menu for what the removed buttons carried, and
Glass promoted from an always-on backdrop to a real theme.

Two things worth remembering:

- **`font-synthesis: none` silently disabled italics.** Segoe UI Variable ships
  no italic face, so `*emphasis*` rendered upright with no warning anywhere —
  invisible to a typecheck and to a colour measurement, and only caught by
  reading the rendered text. Now `font-synthesis: style`.
- **A9 was recorded wrong.** `copy_item` had handled images all along; only the
  click affordance was missing. Rows here are claims, and they get re-checked
  against the code before being worked.

**2026-09-19 (later) — verification pass, and the tooling that makes it possible.**
Four surfaces seen for the first time: the overflow menu, the settings sheet,
the empty state and Glass. Glass genuinely works — the desktop reads through
the panel and its cards.

The day's real lesson was that *looking* is harder than it sounds, and three
separate things were quietly lying:

- **`sed -i` breaks Vite HMR.** It replaces the file by rename rather than
  writing in place, the watcher misses it, and Vite keeps serving the old
  module while still printing `hmr update` for every save. Two captures were
  of stale UI before this surfaced. Edit in place.
- **A hidden window returns its last painted frame.** `PrintWindow` on a panel
  the app has blur-hidden is not an error, it is yesterday's picture. Always
  show the window first (`shot.ps1 -Show`).
- **`MainWindowHandle` is not the panel.** A Tauri process owns a crowd of
  helper windows — single-instance, tray, global-hotkey, IME — and Windows
  names one of those as the main window; in a debug build the largest visible
  one is a 1200x618 helper. The app's real windows are exactly those of class
  `Tauri Window`.

`scripts/` now carries `shot.ps1` (capture, `-Show`, `-Screen`, `-Title`),
`drive.ps1` (click and type, refusing to click at a hidden window's zero rect)
and `stop.ps1` (stop without the guardian resurrecting the app).

**The stylesheet was quietly broken and a screenshot could not show it.** The
block-replacement regex used while rebuilding the panel was not anchored, so
`^\.search` matched `.search-field {` and ate its rule; five selectors ended up
defined twice, and CSS being last-wins, the *stale* copy won each time — the
intended selection ring and search pill were never what was on screen, and the
captures looked close enough to pass. `scripts/css-audit.mjs` now checks for
duplicate selectors, undefined custom properties and unused classes, and
`bun run build` fails on the first two.

**2026-09-19 (later still) — drawing, the composer menu, and a driver that
actually clicks.** A7 and A8 are done and verified end to end: plus-menu,
canvas, three shapes in three colours, Save, and the drawing arriving in the
panel as an item.

**No synthetic click had worked all day, and every failure looked like
something else.** `SendInput` reported success and the cursor appeared to move,
so the blame kept landing on focus, on z-order, on WebView2. The cause was a
PowerShell value-type trap: `$input.mi.dx = …` mutates a *copy* of the nested
struct field, so every event went out with all fields zeroed. SendInput accepts
that happily and returns 1. The cursor "moving" in the first test was a false
positive — it was already at the target point from a preceding `SetCursorPos`.
Build the inner struct as its own variable and assign it whole.

Two smaller ones from the same stretch: two PowerShell scripts could not be
chained in one session because both declared a type called `Win32` and
`Add-Type` refuses the second, leaving its functions silently method-less; and
the composer's plus button, placed *beside* the field as a 42px control, took
enough width to clip the placeholder — Copper puts it inside the field, which
is both the faithful answer and the one that fits.

**2026-09-19 (evening) — settings becomes a window.** D10 built and verified.
The sheet is gone; `src/settings/` holds the window, the primitives and one
file per pane, which is the shape the sixty-odd B rows need to land in.

**A real bug fell out of finally seeing the data.** Every model in the Hugging
Face cache reported its size as `—`. `DirEntry::metadata()` does not follow
symlinks, and a Hugging Face snapshot entry is a link into `blobs/` — so the
scan was measuring the link, not the model. `fs::metadata` follows it: the two
cached Qwen3 files now read 204 MB and 767 MB. The symlink layout was written
down in this very thread the night before; the code still got it wrong, and
only looking at the rendered pane caught it.

Two smaller notes from the same pass. `.keys` — the shortcut recorder's button
— had **no CSS rule at all** and had been rendering unstyled; the audit's
unused-class list is what surfaced it, from the other direction. And a `Note`
placed inside a `Group` renders inside the card, where it reads as a setting
rather than as commentary; `Group` now takes a `note` that sits above the card.

**2026-09-19 (night) — dismiss on blur, and credits removed.**

The panel now dismisses itself when you move on. It is driven by the native
window's focus event rather than a blur listener in the webview, because the
webview is not reliably told when the native window loses activation — which is
precisely the case that matters, a click into another application.

Three things had to be wrong at once for this to look simple:

- **`hide()` from a spawned thread silently does nothing.** Window operations
  belong on the main thread on Windows; called from a timer thread it returns
  `Ok` and has no effect.
- **Asking our own windows `is_focused()` does not work.** Queried during a
  focus transition it answers about the most recently active window, so a
  foreign window taking focus read as "one of ours has focus" and the panel
  stayed. Asking the OS which *process* owns the foreground window is
  unambiguous, and covers windows we have not built yet.
- **The test tooling was invalidating the test.** `shot.ps1 -Show` and
  `drive.ps1` raise a window with `ShowWindow`, which Tauri knows nothing
  about: it still believes the window is hidden, so a later `hide()` is a
  no-op. Every "auto-hide is broken" reading came from a window shown behind
  Tauri's back. Behavioural tests must summon the panel the way the app does.

Credits are out of About. A paid product does not carry a list of the things
it learned from in its settings, and Apple would not ship one. The licence
position is unchanged because bard ships no code from either reference — with
one exception now closed: the markdown tokenizer's regex was character for
character cooper's, and cooper is Apache-2.0. It is now an independent scanner,
which also states marker precedence explicitly and leaves an unclosed marker as
plain text instead of swallowing the rest of the line.

**2026-09-19 (late) — the second engine, and what it actually costs.**

`transcribe-rs` with the `onnx` feature is now a dependency beside
`transcribe-cpp`. That was the easy half, and the hard half turns out not to be
the crate at all:

- **An ONNX model is a directory, not a file.** `ParakeetModel::load(dir,
  quantization)` wants a folder of encoder/decoder/tokenizer files. bard treats
  a model as a single path in acquisition, in storage, and in the loader.
- **Nothing in those files says which family they belong to**, so the loader
  has to be told. That makes the curated catalog (B1) load-bearing rather than
  decorative: it is where a model's architecture is recorded. **B5 depends on
  B1**, which was not obvious from the outside.

**B18 is blocked, and it is a build problem rather than a code one.** Any
non-default feature set on `transcribe-cpp` makes `transcribe-cpp-sys` compile
ggml from source, and that build does not come up here. Two obstacles, both now
in `docs/BUILD.md`: CMake is installed but on nobody's PATH, and the crate
shortens paths through a symlink under `%LOCALAPPDATA%	cs` whose target
directory CMake cannot write into. The acceleration story — Vulkan, per-ISA CPU
dispatch, the selector — all sits behind that one build.

**2026-09-19 — `transcribe-rs` builds, and brings a shipping hazard with it.**
The crate compiles into bard and the app still starts, so the ONNX path is
open. But `ort`'s default features downloaded pyke's prebuilt ONNX Runtime —
`DirectML.dll` now sits beside the binary as a symlink into
`%LOCALAPPDATA%\ort.pyke.io\dfbin\…`.

That is precisely the binary Handy's manifest refuses, and its reasoning
applies to us exactly: the prebuilt is compiled with a global `/arch:AVX2`
baseline and runs BMI2/AVX **in a static initialiser**, so it crashes at
process start on any pre-Haswell CPU. It works on this machine and would fail
at launch on a customer's older one — the worst shape a bug can have in a paid
product, because nothing here will ever show it.

bard cannot disable it from its own manifest. `transcribe-rs` declares
`ort = { version = "=2.0.0-rc.12", optional = true }` with default features
left on, and Cargo features are additive, so `download-binaries` cannot be
removed from the outside. The fix is the one Handy uses: supply a baseline
ONNX Runtime and set `ORT_LIB_LOCATION` at build time. Logged as **E0, and it
blocks shipping** — not because it is urgent today, but because an installer
built from this tree right now would crash for a stranger.

**2026-09-19 — the catalog (B1).** `src-tauri/src/catalog/` is bard's own
curated list, baked in at compile time: a machine that has never been online
still has something to offer, and updating the list is shipping a build, which
is the right cadence for entries that carry checksums.

It is deliberately not a copy of Handy's `catalog.json`. The *facts* in such a
file — sizes, checksums, what a model can do — are facts, but the curation is
theirs, and we have just taken credits out of About on the grounds that a paid
product does not carry a list of what it learned from. So the schema is ours
and the rows are ours.

`architecture` is the load-bearing field, and the reason this had to come
before B5: it decides which engine loads a model, and nothing in a model's own
files states it reliably. Adding a model is now a row in a JSON file rather
than a branch in the loader.

The Models pane reads like Handy's now — recommendation badge, description,
accuracy and speed as coarse bars, capability tags. The bars are deliberately
bars: a number would imply a precision these scores do not have, since they
rank the catalog against itself rather than against any benchmark. A model
installed from Hugging Face shows no bars at all, because it carries no
curation and empty bars would read as "measured, and found wanting".

**And one thing the catalog made impossible to keep ignoring: every curated
model downloads from `blob.handy.computer`** — Handy's CDN. Fine for a fork,
not fine for something we sell. Logged as **E8, blocking**.
