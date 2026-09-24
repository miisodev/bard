# bard — parity ledger

The build plan. bard ships when every in-scope row below is complete, every
`later` row remains out of the consumer release, and the whole installed product
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

A US$20-per-year native Windows app: Copper's capture/clipboard workspace and
best-in-class local dictation in one keyboard-first panel. The licence renews
annually, covers three activated machines, and is backed by a 30-day signed
offline lease between bounded online checks. A 14-day feature-complete local
trial needs no account. Bard ships through two Windows channels, one package
each: a signed NSIS installer downloaded from miiso.dev, and an MSIX package
through the Microsoft Store, which Microsoft signs and updates. There is no
downloadable MSIX. The direct channel requires a publicly trusted code-signing
certificate, which is hardware-bound (USB token or cloud signer).

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

Measured 2026-09-24. The original 2026-09-19 inventory remains in the dated log
as a baseline; these figures describe the current working tree.

| | bard | cooper | Handy |
| --- | --- | --- | --- |
| Source LOC | 11,069 | 6,393 | 43,952 |
| Tauri commands | 64 | 40 | 53 |
| User-facing settings panes | **8** | ~15 | **62** |

bard is larger than the 2026-09-19 skeleton while still owing the surface of
both references. The model and post-processing paths have code
implementations; the current product mandate adds a separate set of licence,
surface, lifecycle and release rows below. Blueprint reconciliation changes the
contract, not the implementation status: no row is `done` because a document was
updated.

---

## A. Copper parity — the capture/clipboard half

Verified against the Copper demo videos and cooper's source.

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| A1 | **Done-state checkboxes** — hollow circle to filled check + strikethrough | **done** | `done` column, `set_done`/`clear_completed`, Space toggles the selection off its active item. Favorites survive behind the overflow menu as a filter |
| A2 | **Inline markdown** — bold, italic, strikethrough, code; underscore emphasis deliberately unsupported | **done** | `src/markdown.tsx`; composes with search highlighting so a hit inside emphasis still marks. All four forms verified on screen |
| A3 | **Glass theme** — DWM acrylic, alongside System/Light/Dark | **removed by decision** | Built and verified — the desktop genuinely read through the panel — then cut. Copper offers it; bard does not, because a translucent panel over arbitrary desktop content cannot be made to hold contrast, and a theme that looks wrong over the wrong wallpaper is not one to sell. Themes are System / Light / Dark. `glass.rs` and `window-vibrancy` are gone with it |
| A4 | **Light theme quality** | **done** | Rebuilt off the measured Copper values. Panel `#e6e7ea` vs Copper `#e5e6e9`, card `#f9f9fb` vs `#f8f8f8`, field `#f3f4f7` exact |
| A5 | **Sub-branches / nesting** — `add_child`, branch style, per-item collapse | **missing** | cooper: `BranchPanel.tsx`, nesting to 10 deep |
| A6 | **Section collapse** — `##` clears the board, others compress to one-line dropdowns | **missing** | `set_section_collapsed` absent |
| A7 | **Drawing** — persistent Draw icon to a companion canvas to a retained timeline item | **partial** | Select / pen / line / arrow / rect / ellipse / text; an ink swatch that follows the page, four hues and a custom colour; three widths; undo, redo, Copy and Save. The page follows the theme or takes Light, Dark or a custom colour, and exports carry it. Fullscreen (button or F11 while focused) runs edge to edge. Save durability, focus restoration after save, and the installed 150% receipt remain |
| A8 | **Composer plus-menu** — "Attach" only | **done** | The composer plus-menu offers **Attach** only; drawing lives behind the persistent header Draw icon. Verified in the installed build 2026-09-24 |
| A9 | **Click thumbnail to copy image to clipboard** | **done** | The row was wrong: `copy_item` already handled images, only the click affordance was missing. Card images and attachment thumbnails now copy on click |
| A10 | **Copy as List** — multi-select to Markdown list (`Ctrl+Shift+M`) | **verify** | The list-copy action is implemented and keyboard-complete; the final shortcut collision and installed interaction still need a receipt. |
| A11 | **Shortcuts sheet** (`Ctrl+/`) and fixed shortcut reference | **done** | Settings → Shortcuts is a read-only reference for fixed gestures and panel keys; the customizable chord editor, persisted bindings, and global-shortcut registration layer have been removed. |
| A12 | **Focus hand-back** — Esc returns focus to the previous app | **verify** | `panel.rs` remembers and restores the previous foreground window; still untested by hand |
| A13 | **Right-Shift x2 toggles panel** (Left-Shift x2 captures) | **done** | One shared raw-input coordinator, same-side timing/dirty rules, synthetic-input guard, and deterministic gesture tests; packaged Windows runtime remained responsive through both gestures |
| A14 | **Obsidian export** — vault detection, one file per section | **missing** | cooper has all three commands |
| A15 | **Merge notes** | **missing** | Named on Copper's own feature list |
| A16 | **Update checker** + in-app update banner | **partial** | Tauri updater discovery/install commands and signed-artifact generation exist; the public manifest, Authenticode-signed installer, restart/rollback and clean-machine update tests remain. |
| A17 | **Central sound preferences** — master toggle/volume plus Capture, Copy, Dictation start/stop and Error cues | **partial** | A small shared cue registry now drives capture, copy, dictation and error tones in the panel; native hidden-panel delivery, master volume and per-event settings remain. |
| A24 | **Dismiss on losing focus** — the panel goes away when you move on | **done** | Driven by the native window's focus event, not a webview blur listener: the webview is not reliably told when the *native* window loses activation, which is the case that matters. A 140 ms grace absorbs focus flicker, and the panel stays put when the foreground window belongs to bard, so opening a companion does not yank it away |
| A18 | **Empty state** — contextual, uses the active section name | **done** | Asks "What should we build in &lt;section&gt;?" over a quiet glyph, with a first-run hint about double-shift. Verified, then centred and the hint re-cut to stop it orphaning a word |
| A19 | **No window chrome** — frameless, keyboard-dismissed | **done** | Titlebar row removed |
| A20 | **No branding in the panel** | **done** | The wordmark remains absent. The target header adds persistent Draw and Settings actions without adding a wordmark or title row |
| A21 | **No timestamps on cards** | **done** | Cards keep the direct Copy affordance required by A27; the broader per-card action-row prohibition is retired. Other actions remain keyboard and context-menu routes |
| A22 | **Section header style** — small-caps letterspaced label + hairline rule | **done** | |
| A23 | **Composer copy** — `Add a note or a prompt` | **done** | `#`/`##` still work; the active section name is appended in lowercase parentheses |
| A25 | **Persistent Draw and Settings header icons** | **partial** | Paintbrush and Settings controls now have labels, tooltips and keyboard focus states; the fixed global `Right Alt` / `Left Alt` gesture routes and installed visual receipt remain. |
| A26 | **Companion windows** — dock left/right, exact panel height, DPI/work-area recompute, one visible companion | **verify** | Companions are frameless, always-on-top and skip the taskbar, so their outer rectangle is what is drawn. Measured in the installed build 2026-09-24 at 100%: panel and Settings tops both y=97, heights both 620, visible gap 8 px (DPI-scaled); opening Draw hid Settings. The previous framed windows ended 7 px short with a 15 px gap because placement used the outer rect, invisible resize border included. 150% and multi-monitor receipts remain |
| A27 | **Copy button on items** | **done** | Copy sits in the card's own column (26 px target), so text never runs under it; it surfaces on hover, focus, selection and the active card. Captured in the installed build 2026-09-24 |
| A28 | **Active item treatment** | **verify** | Native `activeItemId` and an aria-current/active class now distinguish the active row from multi-selection; installed keyboard and visual evidence remains. |
| A29 | **7-Zip-like in-app menu** | **verify** | Grouped actions, role semantics, initial focus, arrow/Home/End navigation, typeahead, Escape handling, pointer clamping, disabled states and focus restoration are implemented; installed QA remains. |
| A30 | **Explorer `Send to Bard`** | **partial** | **The verb was never registered until 2026-09-24**: the hooks file used electron-builder's `customInstall`/`customUnInstall`, which Tauri never inserts, so `HKCU\Software\Classes\*\shell\BardSend` was absent on an NSIS-installed machine. Renamed to `NSIS_HOOK_POSTINSTALL`/`POSTUNINSTALL`; the installed build now registers `"…\bard.exe" "%1"`. Direct installer only — the Store copy has no verb. Cold/warm, multi-file, unreadable and uninstall receipts remain |
| A31 | **Independent clipboard and dictation lifespans** | **partial** | Origin-aware age cleanup, favourite/manual exemption, file-safe pruning, independent 1/7/30/90/180-day-or-Forever controls and Prune now exist; the full crash-safe matrix and visual receipt remain. |

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
| B6 | **Transcription history** + independent retention period | **partial** | Dictation inserts carry a `dictation` origin, persist only after verified clipboard output, and use the independent 30-day default/lifespan control; the full real-speech and retention matrix remains. |
| B7 | **Post-processing via LLM** — provider, base URL, API key, model, prompt library | **partial** | The fixed OpenRouter text-only service, bounded/cancellable metadata test, keyring v1 Windows Credential Manager storage, separate Post-processing pane, first explicit model ID, non-streaming request, no-fallback provider routing, response identity checks, and local fallback are implemented. A disposable Windows Credential Manager round-trip and a live metadata-only request passed; the full transcript post-processing round trip remains pending. There is no user-editable endpoint or prompt library. |
| B8 | **VAD** — enable, backend selector (earshot/silero), smoothing | **partial** | `dictation/vad.rs` exists; no settings |
| B9 | **Paste method / clipboard handling / paste delay / reliable paste** | **partial** | Clipboard writes are verified before recording own-write suppression or pasting; text/image selection capture restores the prior text/HTML/image/file snapshot. Full custom-format and locked-clipboard Windows matrix remains. |
| B10 | **Auto-submit** (+ key) | **missing** | |
| B11 | **Append trailing space** | **missing** | |
| B12 | **Custom words / vocabulary** + correction threshold | **missing** | |
| B13 | **Filler-word removal** + custom filler list | **partial** | Filler cleanup exists; not configurable |
| B14 | **Translate to English** / language selector | **missing** | |
| B15 | **Mute while recording** / output device / volume | **missing** | |
| B16 | **Always-on microphone**, channel selector | **missing** | |
| B17 | **Model unload timeout** | **partial** | Fixed 5-minute idle unload; not configurable |
| B18 | **Hardware acceleration** — GPU backend + runtime CPU-ISA dispatch, and a selector | **blocked on the build** | The change itself is one line: on Windows x86_64, `transcribe-cpp` with `features = ["dynamic-backends", "vulkan"]` — a Vulkan GPU backend plus per-ISA CPU modules scored at runtime. But any non-default feature set makes `transcribe-cpp-sys` compile ggml from source, and that build does not come up on this machine: it shortens paths through a symlink under `%LOCALAPPDATA%	cs` pointing at a per-build `out/` directory, and CMake cannot write into it (`os error 267`, then `Unable to open check cache file for write`). CMake also is not on PATH — see `docs/BUILD.md`. Solve the from-source build first; it is the gate for the whole acceleration story. (ONNX Runtime stays CPU-only on Windows on purpose — DirectML pulled a prebuilt with an AVX2 static initialiser that crashes pre-Haswell CPUs) |
| B19 | **Recording overlay** — position, style, on/off | **partial** | A compact, centered pill with six persisted placements (top/bottom × left/middle/right), DPI-aware and clamped. It now follows the theme: the overlay window reads the narrow appearance record its capability allows, and a hairline gives the light pill an edge. On/off remains future work |
| B20 | **Shortcut activation mode** (hold vs toggle) + hold threshold | **done** | Same-key Ctrl+Ctrl has a 50 ms activation grace, 300 ms push-to-talk threshold, short-tap lock, ownership tokens, and cooperative `transcribe-cpp` cancellation. |
| B21 | **Central audio cues** — capture/copy/dictation start-stop/error, master volume, per-event toggle | **partial** | A shared bounded Web Audio cue registry drives the visible panel's capture, copy, dictation and error events; native hidden-panel playback, volume, per-event toggles and visual-equivalent coverage remain. |
| B22 | **Start hidden / show tray icon / autostart** | **partial** | Direct copy: Run-key autostart in General. Store copy: the manifest's `BardStartup` startup task, switched in Settings › Apps › Startup (General opens that page); a packaged launch within two minutes of logon stays in the tray — a workaround, named as one. Start-hidden and tray-icon toggles still missing; the Store startup task is unverified until a Store install exists |
| B23 | **Debug pane** — log level, live log viewer, paths, keyboard diagnostic | **missing** | |
| B24 | **What's new on update** | **missing** | |
| B25 | **i18n** — app language | **missing** | Handy ships a full i18n layer. *Scope call needed* |

## C. Cohesion — where bard earns its own existence

Neither reference does this; it is the reason bard is one app and not two.

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| C1 | Dictation lands in the panel when open, pastes into the focused app when closed | **verify** | |
| C2 | Dictate directly into the composer (voice to note) | **missing** | |
| C3 | Dictate into an existing card (voice to edit / append) | **missing** | |
| C4 | One coherent shortcut map across both halves, no collisions | **done** | Fixed Left/Right Shift, Left/Right Alt, and same-key Control gestures share one raw-input coordinator; Right Shift × 2 + 1/2/3 is reserved for item paste, and no user-customized binding map remains. |
| C5 | One settings surface where dictation and clipboard read as one product | **missing** | |
| C6 | One onboarding covering both halves | **partial** | The onboarding now reaches model setup, feature/keybind summary, trial/licence activation and startup choice; clean install and every state still need visual QA. |
| C7 | One semantic audio-cue service shared by capture, copy, dictation and errors | **partial** | The visible shell now uses one semantic cue registry; native delivery and the complete accessibility/equivalence contract remain. |

## D. Apple-standard polish

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| D1 | **Design system** — tokens, type scale, spacing, motion, both themes | **done** | `src/styles/tokens.css`: type, space, shape, motion, four themes on two axes, reduced-motion. A legacy-alias block at the end shrinks to nothing as components migrate |
| D2 | **Motion and depth** — panel in/out, card insert, state changes; measured, never bouncy | **partial** | Tokens and per-control transitions are in. The panel's own show/hide and list reordering still need a pass. **The panel also has no drop shadow**, and cannot simply be given one: a window-frame shadow makes save/restore of the window size non-idempotent and the panel grows every launch. A real shadow has to be drawn by the web layer into a transparent margin *inside* the window, which means the app no longer fills its own window — a deliberate change to layout, positioning and the resize handles |
| D3 | **Startup** — cold boot time, no flash, no crash | **partial** | Deliberate termination no longer resurrects because the guardian is opt-in; the underlying WebView2 crash investigation and clean-machine cold/warm evidence remain. |
| D4 | **Dictation engine** | **resolved** | One engine: `transcribe-cpp`, which loads the shipped `whisper`, `parakeet`, and `qwen3_asr` catalogue entries as single GGUF files. No ONNX runtime is linked; see B5 and E0 for why the second engine was removed. Acceleration (`dynamic-backends`, `vulkan` on Windows x86_64) is a feature flag away and gated on B18's from-source build |
| D10 | **Settings is a companion surface** | **verify** | Settings is a frameless docked companion of the panel's material with a SETTINGS title row, a close control and Esc; measured flush with the panel (see A26). Installed 150% and lifecycle receipts remain |
| D9 | **Generate `types.ts` instead of hand-mirroring it** | **missing** | Today a Rust struct change breaks the frontend with no compile error. Handy generates bindings with `specta` + `tauri-specta`. Worth adopting before the surface triples |
| D5 | **Real-audio verification** — the mic has never been exercised | **verify** | CPAL start, bounded recording, VAD silence stop, and no-speech fast exit were exercised on Windows; a human speech transcript round trip remains to be recorded. |
| D6 | **Visual QA of every surface** | **in progress** | Fresh local captures now cover the four-step onboarding shell, panel, exact-height Settings companion, About/licence entry, masked key input and explicit update confirmation. Installed/light-dark/150% and remaining state receipts are still required. |
| D7 | **Accessibility** — keyboard reachability, focus restoration, menu/list semantics, live announcements, contrast, screen reader | **partial** | Required panel chords, active-item navigation, shortcut sheet, menu typeahead/focus restoration, labelled copy, live status, reduced motion and corrected AA token values are implemented; NVDA/200% and installed focus passes remain. |
| D8 | **Empty / error / loading / restricted states everywhere** | **partial** | Onboarding, trial/licence, update confirmation, retention/prune, model, overlay, panel and native restricted commands now have explicit copy and controls; the complete surface matrix is not yet visually verified. |
| D11 | **Licence and trial surfaces** — onboarding, About, active/offline/revalidation/restricted/machine-limit states | **partial** | Four-step onboarding, explicit trial start, masked activation, release/deactivate, revalidation, offline-lease and restricted copy are implemented; the real Polar key lifecycle and machine-limit states still need end-to-end evidence. |

## E. Ship

| # | Item | Status | Notes |
| --- | --- | --- | --- |
| E0 | **ONNX Runtime must not be pyke's prebuilt** | **resolved by removal** | `transcribe-rs` is gone, and `ort` with it, so nothing downloads pyke's prebuilt ONNX Runtime any more. The hazard was real — that build runs AVX in a static initialiser and crashes at startup on pre-Haswell CPUs — but it only ever existed because the dependency was added on a wrong premise (B5). **If an ONNX engine is ever genuinely needed, this row comes back**, and the fix is a baseline runtime via `ORT_LIB_LOCATION` |
| E8 | **Host our own models** | **done** | The catalogue points all three final defaults at the pinned, hash-verified `miisodev/bard-models` release `models-v2`, with exact asset URLs, conversion provenance, model-specific notices, byte sizes, and SHA-256 values. The public release assets match the staged hashes, support `Range` requests, and each exact file passed a Windows `transcribe-cpp` load/transcription smoke test. |
| E9 | **A release panic is a silent death** | **partial** | A local panic hook is installed before Tauri startup, writes bounded timestamped crash logs, preserves the previous hook, and exposes a latest-log reader; the Data pane can load the newest report. A secret-free lifecycle log now records console interrupts, gesture outcomes, audio failures, model phases, and cloud test boundaries. A release-profile probe confirmed the hook writes before abort and captures a backtrace. Native/WebView2 process failures and the operator artifact-retention/field workflow remain unverified. |
| E1 | **Recurring annual licence** — US$20/year, 14-day local trial, three paid activations, signed 30-day offline lease, bounded revalidation | **partial** | CSPRNG installation identity, Credential Manager key/lease storage, replay and temporal checks, key rotation, explicit trial, terminal revocation handling, scheduled bounded refresh, Polar broker routes and activation/deactivation UI are implemented. Live four-device, renewal/refund and checkout evidence remain. |
| E2 | Code signing (SmartScreen) | **partial** | Signing happens inside the Tauri build (`scripts/build-release.ps1`): a certificate thumbprint for a token or store certificate, or a cloud signer's sign command, so `bard.exe`, the installer and its uninstaller are signed before the updater signature is written. The previous post-build PFX script could never have worked — code-signing keys are hardware-bound since June 2023 — and would have shipped an installer holding the unsigned `bard.exe`. No certificate yet |
| E3 | Auto-update channel + signed releases | **partial** | The Tauri updater endpoint, embedded public key, cryptographically verified updater artifact, user-invoked confirmed check/install flow, release workflow and updater signature verification exist. A public immutable manifest, Authenticode-signed NSIS/MSIX artifacts, live upgrade and rollback receipt remain. |
| E4 | miiso.dev purchase + download page | **partial** | The authenticated Bard storefront, annual Polar product/benefits, secure entitlement retrieval, broker routes, legal pages and NSIS/MSIX download paths are implemented locally; production remains fail-closed until signed release assets and checkout smoke tests are complete. |
| E5 | Microsoft Store packaging | **partial** | `scripts/build-msix.ps1 -Store` packages the signed `bard.exe` under the Partner Center identity with a `windows.startupTask`; Microsoft signs it after certification. The app detects its package identity and stands its updater down. The validation package builds; name reservation, listing assets, submission and certification remain |
| E10 | **Signed NSIS consumer package** | **partial** | **One direct package: the signed NSIS installer** with embedded WebView2 bootstrapper, Send to Bard hook and updater signature. The downloadable MSIX is dropped (a sideloaded copy would carry a different identity from the Store copy and never update). Authenticode, clean install/upgrade/uninstall, SmartScreen evidence and legal-notice packaging remain |
| E6 | Licence review — cooper is Apache-2.0, Handy is MIT | **clear, keep it that way** | bard ships **no code from either**. Handy was read for facts — which crates, which features, what its catalog records — and facts carry no licence. cooper was read for approach only; `shapes.ts` and `markdown.tsx` are bard's own. The markdown tokenizer *was* a verbatim copy of cooper's regex and has been rewritten as an independent scanner. Attribution attaches to code, so none is owed today — but it becomes owed the moment anything is vendored, `catalog.json` included |
| E7 | Name/brand check against shadcn's Copper — bard must not read as a clone or a knock-off | **open** | |

---

## Production gate

**Overall: NO-GO.** Bard ships only when every gate below is `pass` against the
signed NSIS release candidate installed on a clean supported Windows machine.
A green build, a development-binary screenshot, a passing unit suite or a
feature marked `done` in source is not a substitute for the gate's evidence.
Any P0/P1 defect, secret exposure, data-loss path, unsigned artefact, dishonest
licence state or unexamined primary surface blocks release.

| Gate | Pass condition | Current state |
|---|---|---|
| **G1 Product truth** | Blueprint 1.2 contradictions, currency, retention, installer and licence registers are clean; every A/B/C row is `done`, `off`, or `resolved` with no `missing`, `partial`, `verify`, `regression`, `blocked` or `open` state; public copy matches runtime behaviour | **blocked** — new mandate rows open; blueprint reconciliation is specification only |
| **G2 Commerce and licence** | Sandbox purchase, US$20 annual renewal, cancellation, paid-through access, refund/subscription loss, 14-day trial, three activations, fourth-machine refusal, local/portal deactivation, replacement key, 30-day offline lease, tamper/replay/clock failures and revalidation bounds all pass on installed builds | **blocked** — local contract is implemented; live Polar lifecycle and installed evidence remain |
| **G3 Security and privacy** | No secret licence key, signed lease, signing private key, OpenRouter key or service credential appears in repo, binary strings, SQLite, events, URLs, logs, crash reports or notifications; keyring round trip passes; CSP/capabilities are least-privilege; signed lease and updater signatures reject tampering; uninstall removes persistent secrets and the shell verb | **blocked** — local lease/updater checks pass, but Authenticode, installed secret inspection and uninstall receipt remain |
| **G4 Installer and distribution** | A signed NSIS installer for direct distribution; a valid MSIX Store submission; clean install, repair, upgrade and uninstall pass for both; no MSI, downloadable MSIX or unsigned public artifact; SmartScreen evidence is recorded; NSIS onboarding and in-app onboarding match the design system; Explorer verb registers, repairs and removes | **blocked** — NSIS and unsigned MSIX candidates are built and checked, but Authenticode, Store certification and installed lifecycle evidence remain |
| **G5 Update and lifecycle** | Signed update discovery/download/install, restart, rollback and failure copy pass; updater pre-exit does not resurrect Bard; deliberate Task Manager exit stays dead; single-instance, autostart and companion lifecycle pass cold/warm | **blocked** — updater artifact and opt-in guardian are implemented; public feed, Authenticode and installed lifecycle evidence remain |
| **G6 Installed visual QA** | Every window, overlay, menu, onboarding step, settings pane and state is captured in light/dark, 100%/150% scaling and at least one narrow display; no clipping, smudge, stale frame, overlap, hidden scrollbar or misaligned companion | **blocked** — local onboarding/panel/settings/About captures exist; installed matrix remains |
| **G7 Keyboard and accessibility** | Every action completes without a mouse; focus never disappears or lands behind a window; menu/list/companion focus order is correct; screen reader announces capture, copy, active/selected state, dictation, errors and licence changes; reduced motion and 200% text pass; rendered contrast meets WCAG 2.2 AA in every state | **blocked** — local keyboard/contrast/focus improvements pass; screen-reader and installed matrix remain |
| **G8 Dictation and audio** | Human speech round trip, silence/no-speech/cancel, 300ms hold/short tap, 20-minute boundary, cancellation, local/cloud fallback and hidden-panel errors pass; cue events are bounded, non-mixing, non-recording and independently adjustable; visual equivalents remain complete | **blocked** — human speech, error delivery, B21 and C7 open |
| **G9 Clipboard, files and retention** | Sentinel capture, own-write suppression, excluded history, Send to Bard cold/warm/multi-file/unreadable/duplicate paths, Copy failures, favourite/manual exemption, each lifespan, crash-safe pruning and orphaned-file checks pass without deleting external data | **blocked** — local ownership/transaction/clipboard fixes pass; installed matrix remains |
| **G10 Reliability and performance** | Cold/warm launch, idle memory, panel resize/move, monitor change, a synthetic 500-item stress fixture (not a retention cap), long timeline, rapid gestures/captures, model switching/download cancellation, network loss and process termination meet documented budgets without deadlock, leak, data loss or ghost process | **blocked** — D2/D3 partial; stress and long-session evidence absent |
| **G11 Diagnostics and support** | Panic, native/WebView2 and updater failures leave bounded redacted evidence; symbols/PDB are retained; latest-report UX works; a release operator can triage, reproduce or escalate from the documented workflow | **blocked** — E9 partial |
| **G12 Legal and commercial** | Terms, Privacy, refund/renewal language, model licences/notices, third-party attributions, brand check and every product-page claim are reviewed against the installed build | **blocked** — E7 open; licence Terms/Privacy absent |

A gate moves to `pass` only with a reproducible receipt against the release
candidate. Evidence belongs in this ledger or its referenced release record, not
in the blueprint bodies.

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
| "Send to Bard" never appeared in Explorer | The installer hooks were named for electron-builder (`customInstall`); Tauri inserts only `NSIS_HOOK_*`, so the verb was defined and never run |
| An empty card in the timeline, and the active model "not installed" | The move to `%LOCALAPPDATA%` skipped any folder that already existed locally — `images` and `models` are created at startup — stranding every image and model in Roaming while image rows kept Roaming paths the new containment check refuses. Folders now merge file by file and stored paths are rebased once their file has moved |
| An image card could come up "Image unavailable" at launch | The image reader took its folder from state that setup() registers, and the panel asks for images before setup runs; the folder is now resolved directly |
| Ctrl+Shift+C stopped copying in terminals and DevTools | It was Bard's default global capture chord. Chords now start unassigned; the gestures are the defaults |
| Ctrl+C produced no useful result | Console control was previously allowed to terminate the process and its guardian through the OS default handler. Bard now records a clean console-interrupt lifecycle event, marks the exit, and detaches the guardian process; the OpenRouter test also has a bounded, cancellable UI action. |

## Status vocabulary

**done** verified working · **partial** exists, incomplete · **missing** not built ·
**verify** may exist, unconfirmed · **off** built but wrong against the target ·
**regression** actively breaks the target · **resolved** closed by a settled
architecture or removed requirement · **blocked** release-stopping work ·
**open** decision not yet made · **later** valid outside the current release ·
**in progress** actively being implemented but not yet gated

## Working rules

1. **Nothing is done until it has been looked at.** Capture the window
   (`scripts/shot.ps1`) and inspect it. v0.1.0 shipped unseen; that does not happen again.
2. **Copper over cooper** wherever they disagree.
3. **No feature creep.** Every row traces to the current product mandate,
   Copper, Handy, or cohesion between those products.
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
values, model-specific licences, and the pinned first-party
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
also pass. The final package pass emits the then-current dual-installer set and keeps
`target/release/bard.pdb` beside the release binary for symbolication; E10 now
retires every consumer format except signed NSIS. A real
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

**2026-09-23 — recording overlay refinement.** The installed native overlay was
captured and corrected: the clipped border/shadow treatment was removed, the
window was reduced from 320×96 to 260×60, the pill is centered in the
transparent stage, the cancel affordance is borderless, and the level bars now
animate with transforms rather than layout properties. The installed binary
was recaptured after the change; the pill reads as one clean surface with no
hard edge or smudge.

**2026-09-23 — six-position overlay control.** Dictation settings now persist a
vertical Top/Bottom choice and a horizontal Left/Middle/Right choice, composing
six valid placements. The native positioner follows the monitor under the
cursor, uses a DPI-aware 40-pixel edge margin, clamps on small displays, and
repositions an already-visible overlay immediately. Focused Rust position tests
cover all six values and the overflow case; the frontend typecheck, CSS audit,
and bundle pass. The rebuilt NSIS binary was installed and exercised through
Settings → Dictation: `top-left` persisted and rendered at `(40, 40)`,
`bottom-right` at `(1300, 800)` on the 1600×900 test display, and the default
`bottom-middle` at `(670, 800)`. The process stayed responsive and the overlay
was recaptured at each representative placement; the user setting was restored
to `bottom-middle` after the test.

**2026-09-23 — longer takes and sentence-aware cleanup.** The bounded audio
path now retains exactly 20 minutes of 16 kHz mono samples, stops at the
boundary as a normal take, and continues into local transcription instead of
misreporting a microphone failure. The OpenRouter system instruction now
requests faithful sentence-boundary repair, natural paragraph flow, and
careful preservation of names, numbers, technical terms, quotations, lists,
language, emphasis, and uncertainty while forbidding summarization or
instruction following. The completion budget is bounded at 8,192 tokens for a
complete long take; the provider request now has a 45-second timeout inside a
60-second cancellable deadline. Prompt contract and cap-boundary tests are part
of the Rust suite. The rebuilt NSIS installer completed successfully and the
installed binary remained responsive on a clean launch.

**2026-09-23 — product mandate reconciled into the canonical library.** The five
blueprints now specify the US$20 recurring annual licence, 14-day local trial,
three paid activations, Ed25519-signed 30-day offline lease with bounded online
revalidation, signed NSIS + MSIX direct distribution, Microsoft Store MSIX
submission, persistent Draw/Settings
header actions, panel-height companion docking, item Copy, one active item, a
keyboard-complete desktop-style menu, the narrow Explorer `Send to Bard` verb,
independent clipboard/dictation lifespans, one central five-cue audio service and
an all-or-nothing Apple-level production gate. Old rows that implement the
superseded standalone Settings/Draw, composer Drawing route, hidden 500-item
prune or capture-only sound preference are marked `off`/`partial`; missing work
is added rather than inferred complete. The production gate is **NO-GO** until
the installed signed release candidate supplies every receipt.

**2026-09-24 — two-channel distribution and tools hub.** The release mandate now
covers Microsoft Store MSIX plus direct signed NSIS/MSIX downloads. A Windows
SDK-backed MSIX packager now builds and structurally validates a desktop
full-trust package from the same release binary; the release workflow requires
both package formats and the site exposes separate NSIS/MSIX delivery paths.
Bard and BrainLLM now live under `/tools`, with dedicated product pages and
storefront-style cards; the legacy `/saas/bard` and `/bard` routes redirect to
`/tools/bard`. Authenticode signing, Store certification, and live commerce
remain **NO-GO**.

**2026-09-24 — review pass, distribution fixes and the companion redesign.**
A review of the NOTES work against the installed build found the Explorer verb
never registered, the signing plan unbuyable (a PFX cannot be issued since June
2023), the Store and direct MSIX conflated under one identity, no packaged-app
awareness, a Yes/No dialog in the install section that stopped every passive
update, and an upgrade migration that stranded images and the active model in
Roaming. All fixed and re-verified on the installed build except what needs a
certificate or a Store install. Miiso's rulings: drop the downloadable MSIX
(the Store is the only MSIX), keep onboarding in the app, and make the surfaces
gorgeous and minimal. Settings and Draw are now frameless companions of the
panel's material, flush with it; Copy has its own column; the active card is a
soft ring; scrollbars appear only on hover; actions are one word (Draw, Refine,
Check, Read, Buy, Record, Reset); chords start unassigned with the gestures as
defaults; trial and restricted licences can reach the purchase page, and every
in-app link now opens the browser (there was no opener, so none did). The
dictation overlay was not re-verified: it has no preview path, and triggering
it records and pastes into the focused app.

**2026-09-24 — fixed command surface and standalone companions.** Right Shift
× 2 now accepts a quick `1` / `2` / `3` suffix to paste the first three timeline
items into the focused app, while an unadorned Right Shift × 2 still toggles the
panel after the slot window. Right Alt × 2 opens Settings and Left Alt × 2 opens
Draw as single companion windows; the tray/header paths retain their existing
side-by-side behavior. Custom shortcut recording, persisted keybinds, the
global-shortcut plugin, and its generated permissions are removed. Draw now has
a fullscreen control beside close and responds to F11. Gesture, Rust, Clippy,
and frontend build gates pass.

**2026-09-24 — Draw's page, standalone windows, and an overlay that follows
the theme.** Draw's page is now a choice — Light, Dark or a custom colour — that
follows the theme until made, and exports carry it; the first swatch is the ink,
which follows the page, and a custom pen colour sits after the hues. The panel
background is reduced to a single custom colour plus Match theme, and no longer
reaches Draw, whose page has its own background. F11 moved from the global
input hook, which claimed it whenever Draw was merely visible — so a browser in
front toggled too — to Draw's own keys while it has focus; `Esc` steps out one
layer at a time. A companion opened on its own is now an ordinary window: in
the taskbar, not on top, not dismissed when you switch apps (a Settings window
used to vanish mid-way through copying an API key), and movable by its title
row. The dictation overlay never followed the theme because its capability
file denied the state read its theme hook depended on; every window now reads
a narrow appearance record instead of the whole app state (clipboard history
included). Verified in the installed build: canvas sheet, Dark page with light
ink, custom wells, fullscreen edge to edge, Esc out of fullscreen, Match theme.
Not verified here: gestures by physical key — injected Alt taps put Windows'
menu mode on the focused window and are not a fair test.
