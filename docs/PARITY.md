# bard — parity ledger

The build plan. bard ships when every row below is **done** and the whole thing
survives the two tests:

**The specification lives in `docs/blueprints/`** — five documents describing
the finished product. This ledger is the plan for getting there and tracks
status; the blueprints carry no status and never will. Where the two disagree,
the blueprints are the product and this file is behind.

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

Dictation targets Wispr Flow's *capability* (minus notetaking) but delivers local
transcription on open weights, with genuine freedom of model choice — choose a
curated Bard role, use a compatible file already in the shared Hugging Face
cache, or inspect a public Hugging Face repository — without the configuration
sprawl that would lose a non-technical user. Handy's curated recommendation list
is the pattern to follow.

## Reference material

| Source | What it is | Where |
| --- | --- | --- |
| **shadcn's Copper** | **The target.** Mac app, closed source | `~/Downloads/Cooper/*.mp4` (3 demos), shadcn.com/copper |
| `cooper/` | TouchMyBar/cooper — Apache-2.0 open-source Copper recreation, v0.3.75. Code reference, *not* the target | fork of `TouchMyBar/cooper` |
| `Handy/` | Handy — dictation app, forked for reference. Also installed on this machine | `OneDrive/Desktop/Handy/handy.exe` |
| `bard/` | us | `miisodev/bard` |

**cooper is a means, Copper is the end.** Where cooper deviates from Copper, Copper wins.

## Where we actually stand

Measured 2026-09-23. The original 2026-09-19 inventory remains in the dated log
as a baseline; these figures describe the current working tree.

| | bard | cooper | Handy |
| --- | --- | --- | --- |
| Source LOC | 11,069 | 6,393 | 43,952 |
| Tauri commands | 64 | 40 | 53 |
| User-facing settings panes | **8** | ~15 | **62** |

bard is larger than the 2026-09-19 skeleton while still owing the surface of
both references. The model and post-processing paths have code
implementations; release verification and the rows marked `partial` or
`verify` remain below.

---

## A. Copper parity — the capture/clipboard half

Verified against the Copper demo videos and cooper's source.

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| A1 | **Done-state checkboxes** — hollow circle to filled check + strikethrough | **done** | `done` column, `set_done`/`clear_completed`, Space toggles the selection off its lead item. Favorites survive behind the overflow menu as a filter |
| A2 | **Inline markdown** — bold, italic, strikethrough, code; underscore emphasis deliberately unsupported | **done** | `src/markdown.tsx`; composes with search highlighting so a hit inside emphasis still marks. All four forms verified on screen |
| A3 | **Glass theme** — DWM acrylic, alongside System/Light/Dark | **removed by decision** | Built and verified — the desktop genuinely read through the panel — then cut. Copper offers it; bard does not, because a translucent panel over arbitrary desktop content cannot be made to hold contrast, and a theme that looks wrong over the wrong wallpaper is not one to sell. Themes are System / Light / Dark. `glass.rs` and `window-vibrancy` are gone with it |
| A4 | **Light theme quality** | **done** | Rebuilt off the measured Copper values. Panel `#e6e7ea` vs Copper `#e5e6e9`, card `#f9f9fb` vs `#f8f8f8`, field `#f3f4f7` exact |
| A5 | **Sub-branches / nesting** — `add_child`, branch style, per-item collapse | **missing** | cooper: `BranchPanel.tsx`, nesting to 10 deep |
| A6 | **Section collapse** — `##` clears the board, others compress to one-line dropdowns | **missing** | `set_section_collapsed` absent |
| A7 | **Drawing** — composer plus-menu to "Create Drawing…" to canvas window to an item in the panel | **done** | Own window (`draw.html`), native Windows chrome because it is a document window rather than a summoned overlay. Select / pen / line / arrow / rect / ellipse / text, five colours, three widths, undo, redo, delete, Save and Copy and Save. `src/draw/shapes.ts` is a fresh pure model — no React, no Tauri — rather than a copy of cooper's, which is Apache-2.0 and would have entangled a paid product. Whole round trip verified on screen |
| A8 | **Composer plus-menu** — "Attach Files…" / "Create Drawing…" | **done** | The plus sits *inside* the field, as in Copper — beside it, it stole enough width to clip the placeholder. Shared `Menu` component with the header overflow |
| A9 | **Click thumbnail to copy image to clipboard** | **done** | The row was wrong: `copy_item` already handled images, only the click affordance was missing. Card images and attachment thumbnails now copy on click |
| A10 | **Copy as List** — multi-select to Markdown list (`Ctrl+Shift+C`) | **verify** | Multi-select exists; list-copy path unconfirmed |
| A11 | **Shortcuts sheet** (`Ctrl+/`) and shortcuts editor | **done** | Settings → Shortcuts is live; normal chords are recorded transactionally, while fixed gestures are shown separately and cannot be rebound |
| A12 | **Focus hand-back** — Esc returns focus to the previous app | **verify** | `panel.rs` remembers and restores the previous foreground window; still untested by hand |
| A13 | **Right-Shift x2 toggles panel** (Left-Shift x2 captures) | **done** | One shared raw-input coordinator, same-side timing/dirty rules, synthetic-input guard, and deterministic gesture tests; packaged Windows runtime remained responsive through both gestures |
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
| A23 | **Composer copy** — `Add a note or a prompt` | **done** | `#`/`##` still work; the active section name is appended in lowercase parentheses |

## B. Handy parity — the dictation half

The Settings window exposes eight panes; Handy's reference exposes 62 settings.
Every row is a user-visible capability.

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| B1 | **Curated model catalog** — recommended ranking, accuracy/speed bars, language count, model metadata, size, checksums | **done** | `src-tauri/src/catalog/catalog.json` is embedded at compile time and carries the three final roles, exact Q6_K assets, conversion provenance, model-specific licences, byte sizes, full-file SHA-256 values, and `models-v2` host metadata. The public release assets match those bytes and each exact blob passed the Windows `transcribe-cpp` load/transcription gate. |
| B2 | **Model manager UI** — downloaded vs available, search, language filter, per-model Active/Delete, progress with speed + cancel | **partial** | The Models pane now has `Bard models`, `On this computer`, and `Advanced model import`, with explicit download/use/remove, cache check/use/forget, import, progress, speed/ETA when available, cancel, and retry paths. There is no guaranteed model search or language filter; visual and round-trip verification remain pending. |
| B3 | **HF repo paste + one-click install** | **verify** | `owner/repository`, Hugging Face repository URLs, and main-branch resolve URLs are normalized; listing supplies LFS SHA-256, downloads resume and hash the complete file, and installation probes the engine before explicit activation. A live listing check passed; a full HF download/install/use round trip remains to be recorded. Arbitrary non-Hugging-Face direct URLs are rejected. |
| B4 | **Discover models already on the machine** | **verify** | The shared HF cache scan follows snapshot links, checks the exact file with the engine, and supports explicit Use; Forget unregisters an external file without deleting it. No live cache discovery and Use round trip has been recorded. |
| B5 | **Non-whisper architectures** (parakeet, nemotron, moonshine, canary, sensevoice, gigaam, qwen3, voxtral, granite…) | **partial** | `transcribe-cpp` is the sole engine, and the final catalogue now includes `parakeet` and `qwen3_asr` alongside `whisper`, all as single GGUF files. The three selected defaults passed exact-blob Windows load/transcription; the broader Handy architecture catalogue is not present. |
| B6 | **Transcription history** + retention period + limit | **missing** | Handy: `managers/history.rs`, history settings pane |
| B7 | **Post-processing via LLM** — provider, base URL, API key, model, prompt library | **partial** | The fixed OpenRouter text-only service, bounded/cancellable metadata test, keyring v1 Windows Credential Manager storage, separate Post-processing pane, first explicit model ID, non-streaming request, no-fallback provider routing, response identity checks, and local fallback are implemented. A disposable Windows Credential Manager round-trip and a live metadata-only request passed; the full transcript post-processing round trip remains pending. There is no user-editable endpoint or prompt library. |
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
| B20 | **Shortcut activation mode** (hold vs toggle) + hold threshold | **done** | Same-key Ctrl+Ctrl has a 50 ms activation grace, 300 ms push-to-talk threshold, short-tap lock, ownership tokens, and cooperative `transcribe-cpp` cancellation. |
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
| C4 | One coherent shortcut map across both halves, no collisions | **done** | Fixed Shift/Control gestures share one raw-input coordinator; normal chords are conflict-checked and registration is transactional. |
| C5 | One settings surface where dictation and clipboard read as one product | **missing** |
| C6 | One onboarding covering both halves | **partial** |

## D. Apple-standard polish

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| D1 | **Design system** — tokens, type scale, spacing, motion, both themes | **done** | `src/styles/tokens.css`: type, space, shape, motion, four themes on two axes, reduced-motion. A legacy-alias block at the end shrinks to nothing as components migrate |
| D2 | **Motion and depth** — panel in/out, card insert, state changes; measured, never bouncy | **partial** | Tokens and per-control transitions are in. The panel's own show/hide and list reordering still need a pass. **The panel also has no drop shadow**, and cannot simply be given one: a window-frame shadow makes save/restore of the window size non-idempotent and the panel grows every launch. A real shadow has to be drawn by the web layer into a transparent margin *inside* the window, which means the app no longer fills its own window — a deliberate change to layout, positioning and the resize handles |
| D3 | **Startup** — cold boot time, no flash, no crash | **partial** | Guardian self-heals a ~1-in-15 WebView2 startup crash. **A guardian is a patch, not a fix — find the root cause.** It also has a side effect nobody chose: because it relaunches any death inside 20s that left no clean-exit marker, **End Task in Task Manager resurrects the app.** An app that will not stay killed is user-hostile; whatever replaces the guardian must not do this |
| D4 | **Dictation engine** | **resolved** | One engine: `transcribe-cpp`, which loads the shipped `whisper`, `parakeet`, and `qwen3_asr` catalogue entries as single GGUF files. No ONNX runtime is linked; see B5 and E0 for why the second engine was removed. Acceleration (`dynamic-backends`, `vulkan` on Windows x86_64) is a feature flag away and gated on B18's from-source build |
| D10 | **Settings has its own window** | **done** | 760x620, native chrome, sidebar: General / Capture / Dictation / Models / Post-processing / Shortcuts / Data / About. `src/settings/ui.tsx` carries the primitives — Pane, Group, Row, Toggle, Segmented, Select, Button, TextField, Note, Progress — so a new setting is a line of markup rather than a design decision taken again. The old sheet is deleted |
| D9 | **Generate `types.ts` instead of hand-mirroring it** | **missing** | Today a Rust struct change breaks the frontend with no compile error. Handy generates bindings with `specta` + `tauri-specta`. Worth adopting before the surface triples |
| D5 | **Real-audio verification** — the mic has never been exercised | **verify** | CPAL start, bounded recording, VAD silence stop, and no-speech fast exit were exercised on Windows; a human speech transcript round trip remains to be recorded. |
| D6 | **Visual QA of every surface** | **in progress** | Earlier visual passes verified the panel (light and dark), overflow menu, empty state, composer menu, the drawing window and its round trip, and the settings window's General, Models, and Shortcuts panes. The current native pass additionally exercised the fixed gesture surface and Post-processing cancel state; onboarding, the dictation overlay, the section switcher, the context menu, and the Capture / Dictation / Data / About panes remain unseen or need rechecking. |
| D7 | **Accessibility** — keyboard reachability, focus rings, contrast, screen reader | **missing** | |
| D8 | **Empty / error / loading states everywhere** | **missing** | |

## E. Ship

| # | Item | Status |
| --- | --- | --- |
| E0 | **ONNX Runtime must not be pyke's prebuilt** | **resolved by removal** | `transcribe-rs` is gone, and `ort` with it, so nothing downloads pyke's prebuilt ONNX Runtime any more. The hazard was real — that build runs AVX in a static initialiser and crashes at startup on pre-Haswell CPUs — but it only ever existed because the dependency was added on a wrong premise (B5). **If an ONNX engine is ever genuinely needed, this row comes back**, and the fix is a baseline runtime via `ORT_LIB_LOCATION` |
| E8 | **Host our own models** | **done** | The catalogue points all three final defaults at the immutable `miisodev/bard-models` release `models-v2`, with exact asset URLs, conversion provenance, model-specific notices, byte sizes, and SHA-256 values. The public release assets match the staged hashes, support `Range` requests, and each exact file passed a Windows `transcribe-cpp` load/transcription smoke test. |
| E9 | **A release panic is a silent death** | **partial** | A local panic hook is installed before Tauri startup, writes bounded timestamped crash logs, preserves the previous hook, and exposes a latest-log reader; the Data pane can load the newest report. A secret-free lifecycle log now records console interrupts, gesture outcomes, audio failures, model phases, and cloud test boundaries. A release-profile probe confirmed the hook writes before abort and captures a backtrace. Native/WebView2 process failures and the operator artifact-retention/field workflow remain unverified. |
| E1 | Licensing — $30 one-time key issue + offline validation | **missing** |
| E2 | Code signing (SmartScreen) | **missing** |
| E3 | Auto-update channel + signed releases | **missing** |
| E4 | miiso.dev purchase + download page | **missing** |
| E5 | Microsoft Store packaging | **later** |
| E6 | Licence review — cooper is Apache-2.0, Handy is MIT | **clear, keep it that way** | bard ships **no code from either**. Handy was read for facts — which crates, which features, what its catalog records — and facts carry no licence. cooper was read for approach only; `shapes.ts` and `markdown.tsx` are bard's own. The markdown tokenizer *was* a verbatim copy of cooper's regex and has been rewritten as an independent scanner. Attribution attaches to code, so none is owed today — but it becomes owed the moment anything is vendored, `catalog.json` included |
| E7 | Name/brand check against shadcn's Copper — bard must not read as a clone or a knock-off | **open** |

---

## Fixed defects

Kept because each one was invisible from the code and obvious the moment
someone used the app.

| Symptom | Cause |
| --- | --- |
| Switching theme changed only the window you switched it in | `set_theme` wrote the setting but emitted no `refresh`, so no other window re-read it. The drawing canvas was worse — it never applied a theme at all, and the overlay hardcoded `"system"`. All four windows now share one `useTheme` hook that reads on open and follows later changes |
| Settings and the canvas could not be reopened | Closing them *destroyed* them. `get_webview_window` then returned `None` and the open command returned silently. Their close is now intercepted and hides instead |
| Launching sometimes did nothing, and needed a second try | The auto-hide raced the show: summoning a window produces a burst of focus changes as it takes the foreground, and one of those blurs fired the hide 140 ms later. An explicit show now marks the panel for 700 ms, and the auto-hide stands down |
| Opening the app showed nothing | It only revealed the panel during onboarding, so every later launch appeared to do nothing. It now opens the panel unless Windows started it at login — the autostart entry passes `--autostart`, which is the only way to tell the two apart |
| The installed app died instantly on every launch | A panic on `db.rs`'s `unwrap` — and `panic = "abort"` in the release profile turns any panic into a silent exit with no window and no message. Windows declared in `tauri.conf` are created *before* `setup()` runs, so a focus change, the webview asking for state, or the clipboard monitor could all reach the database while it was still `None`. Guarding callers one at a time did not work; the database now opens in `main()` before the builder |
| Windows launched the wrong build at login | Autostart registers whichever executable is running, so a `tauri dev` session repointed the login entry at `target\debug\bard.exe` — a build that only works while Vite is up. Release builds re-assert their own path; debug builds no longer touch it |
| The panel grew on every launch | 380 to 396 to 424 to 440 to 476, untouched. `save_geometry` and `set_size` disagree by the window frame, so each launch restored what it saved *plus* the frame and wrote the larger number back. Latent for as long as the panel existed — invisible while the window had no frame, because then the two were the same number, and exposed by turning the shadow on. Saving a different accessor did not help: both sides of the round-trip are lossy once a frame exists. The window has no frame again |
| A smudge into a hard bottom-right corner | `.app` fills the window exactly and carried `box-shadow: 0 16px 56px`. A shadow with no transparent margin to fall into paints inside the window and is clipped square at the edge — worst at the bottom-right, where both offsets meet. The shadow is the compositor's job; the window now asks for one and the CSS does not draw its own |
| Dictation shortcut made the installed process non-responsive | The old global-key path called Tauri overlay/window operations from the rdev and CPAL threads; Windows WebView2 deadlocked before recording began. All native window operations now queue through `run_on_main_thread`, the CPAL callback is data-only, and a bounded monitor owns silence/error transitions. |
| Ctrl+C produced no useful result | Console control was previously allowed to terminate the process and its guardian through the OS default handler. Bard now records a clean console-interrupt lifecycle event, marks the exit, and detaches the guardian process; the OpenRouter test also has a bounded, cancellable UI action. |

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

**2026-09-19 — B5 was wrong, and checking it cost less than building it.**

The premise was that non-whisper architectures need an ONNX engine, so bard
took on `transcribe-rs`. Before building the acquisition path that premise
implied — ONNX models are directories, so storage and download would both have
had to change — two cheap checks contradicted it:

- `transcribe-cpp` 0.2.3 ships `parakeet.h` and `moonshine_streaming.h`
  alongside `whisper.h`.
- Every one of the **sixty-nine** models in Handy's catalog is a single GGUF,
  across **seventeen** architectures — canary, gigaam, qwen3_asr, voxtral,
  granite_speech, sensevoice, moss, medasr and the rest.

`transcribe-rs` is their legacy ONNX path, not the one in use. So supporting a
family is adding catalog rows, not adding an engine. The dependency is removed,
and **E0 goes with it** — the pyke ONNX Runtime that crashes at startup on
pre-Haswell CPUs was dragged in by a dependency bard never needed.

The qwen3-asr refusal that started this has a better explanation than
"unsupported architecture": the cached model is a *third-party* GGUF conversion
(`unslothai/…`), and the engine reads the conversions Handy publishes. Which
GGUF, not which architecture.

Worth stating plainly, because it is the second time today: the earlier
conclusion was recorded confidently from one failed load, and generalised into
a plan. One contradicting check was thirty seconds of reading a header.

**2026-09-19 — the panel was growing, and the first fix for it was wrong.**

380 to 396 to 424 to 440 to 476, with nobody touching it. `save_geometry` and
`set_size` disagree by the window frame, so each launch restored what it had
saved *plus* the frame and wrote the larger number back.

The first attempt — save `inner_size()` instead of `outer_size()` — was
reasoning rather than measurement, and it did not work: the panel still grew,
and the stored value still tracked the outer rect. Both sides of that
round-trip are lossy once a frame exists, so changing which accessor is read
cannot fix it.

The frame was mine. It arrived with `shadow: true`, added while fixing the
bottom-right corner — but the corner fix was *removing the CSS box-shadow*
that the window edge was clipping square. The compositor shadow was extra, and
it was the part that broke geometry. Off again: four cold launches, 380x620
every time, all four corners clean.

**Cost of the trade, recorded in D2:** the panel now has no drop shadow at all.
Giving it one means drawing it in the web layer into a transparent margin
inside the window — which means `.app` stops filling its own window, and
layout, positioning and the resize handles all have to account for that. Not a
setting to flip.

Two process notes. The fix was reported as done before it was measured, and
the measurement is what showed it was not. And this bug predates today — 380
was already becoming 396 — it simply needed a bigger frame to grow fast enough
to notice.

**2026-09-19 — Glass removed, models moved to canonical upstream.**

Two decisions from Miiso.

**Glass is out.** It worked — verified composited, the desktop reading through
the panel and its cards — and it is still the right call to cut. A translucent
panel cannot hold its contrast over arbitrary desktop content, and a theme that
looks wrong over the wrong wallpaper is not one to sell. Themes are System /
Light / Dark. `glass.rs`, the `data-glass` axis and `window-vibrancy` are gone.

**Models now come from their canonical upstream.** Every catalog URL points at
`ggerganov/whisper.cpp`, pinned to a commit, with each file's sha256 recorded.
Worth noting what the check found: two of the three files bard was fetching
from the third-party mirror were **byte-identical to upstream** — same size,
same hash — so the mirror was only re-hosting ggerganov's reference builds. The
third was a mirror-specific Q4_1 quantisation, replaced with upstream's Q5_0.
Range requests and the pinned revision both verified against the live endpoint
before the switch.

So the bandwidth question is closed for everything bard ships. What remains of
E8 is narrower and honest: **non-whisper families need a GGUF conversion that
upstream does not publish.** Handy converts them; we will not take theirs. That
means a conversion pipeline and our own org — real work, and it gates how wide
the catalog can get rather than whether bard can ship.

**2026-09-23 — final model roles, ownership-safe acquisition, and optional text post-processing.**

The model-manager implementation replaces the superseded Whisper-only plan with
three explicit roles: `whisper-small-Q6_K.gguf` for **Fast multilingual**,
`parakeet-tdt-0.6b-v2-Q6_K.gguf` for **Best English**, and
`Qwen3-ASR-1.7B-Q6_K.gguf` for **Best multilingual**. The embedded catalogue
records the exact Handy conversion revisions, byte sizes, full-file SHA-256
values, model-specific licences, and the immutable first-party
`miisodev/bard-models` release `models-v2` assets. `transcribe-cpp` is the only
engine. Installation does not activate a model: activation loads the exact file
and writes `model_id` only after verification. Downloads resume and hash the
complete file; Bard-managed deletion is ownership- and path-contained; external
Hugging Face cache files are never deleted.

The Models surface is now specified around **Bard models**, **On this
computer**, and **Advanced model import**, with a separate **Post-processing**
pane. OpenRouter uses the fixed official HTTPS endpoint, keyring v1 Windows
Credential Manager, one explicit first model ID (the stored setting may be a
comma-separated list), a non-streaming text-only request, and the untouched
local transcript as fallback. There is no user-editable endpoint, prompt
library, arbitrary non-HF direct URL, or guaranteed model search/filter.

**Verification still open:** exercise live HF/cache install/use and
ownership-safe deletion paths; run a live OpenRouter request round trip with a
real, disposable credential; and verify E9's native-crash/artifact-retention
workflow in a packaged build. E8 and B1 are verified; B3/B4/B7 remain
implemented but not fully release-verified.
The historical entries above remain part of the ledger and describe earlier
checkpoints rather than the current host decision.

**2026-09-23 — final verification pass.** `bun run build`, strict CSS audit,
`impeccable detect`, `cargo check --locked`, `cargo clippy --locked -- -D
warnings`, and all 35 Rust tests pass. The live Hugging Face listing check, the
Windows Credential Manager round-trip, the exact Q6_K load/transcription gate,
the public `models-v2` hash/range checks, and a release-profile crash-hook probe
also pass. The final package pass emits both MSI and NSIS installers and keeps
`target/release/bard.pdb` beside the release binary for symbolication. A real
OpenRouter request and native/WebView2 crash collection remain explicit
follow-up evidence because they require a user credential or a customer failure
environment.

**2026-09-23 — input/incident hardening pass.** The reported installed-build
freeze was reproduced by sending its old dictation shortcut: the process became
non-responsive before transcription because raw-input/audio threads were
calling Tauri windows directly. The fix moves every window operation onto the
UI thread, keeps CPAL side-effect-free with bounded buffers and a normal VAD
monitor, adds terminal idle/error events, cooperative `transcribe-cpp`
cancellation, and a secret-free lifecycle breadcrumb. The shortcut surface is
now two transactional OS chords plus fixed same-key `Left Shift × 2`,
`Right Shift × 2`, and `Ctrl × 2`; Ctrl has a 50 ms activation grace,
300 ms hold threshold, tap-lock behavior, dirty/synthetic-input protection,
and ten deterministic gesture tests. OpenRouter metadata testing is bounded,
cancellable, and reports a safe error instead of spinning indefinitely. The
current native probe stayed responsive through tap and hold paths; human speech
transcription and native/WebView2 crash collection remain open evidence.
