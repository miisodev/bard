# bard — Surfaces

## §0 Using This Blueprint

**Scope.** Every window, sheet and overlay bard draws, specified to
build-readiness. It owns per-surface layout, the zones in order, the exact
user-facing copy and the failure modes belonging to one surface. Rules true of
every surface live in `Design System`; the keys that reach these surfaces live
in `Input & Commands`; what a behaviour *does* lives in `Product & Business`.

**Cross-reference convention.** References to other documents carry the
document name, as in `Design System §4`. A bare §2 is a section of this
document.

**Copy is normative.** Text inside quotation marks in a *Copy* block is the
string that ships, including its punctuation. Typographic quotes and the
ellipsis character are deliberate.

### The entry contract

Every surface below carries the same seven headings, in the same order.

| Heading | What it states |
|---|---|
| **Purpose** | The one job. If it needs two sentences, it is two surfaces |
| **Entered from** | Every route in |
| **Zones** | Outside-in, in visual order |
| **States** | Which of the nine from `Design System §5` occur here, and what each shows |
| **Copy** | Every string, verbatim |
| **Fails when** | What breaks, how it presents, what the surface does |
| **Leaves to** | Every route out |

---

## §1 The panel

**Purpose.** The single window a person works in: everything captured, in
reverse order, searchable, with a composer at the bottom.

**Entered from.** The panel shortcut · a double-tap of Right Shift · the tray
icon · the tray's **Show Bard** · a second launch of the application · a
capture, which shows the panel if it is hidden · **Send to Bard**, which hands
off selected files and shows the panel.

**Zones**, top to bottom, inside the frameless panel window (`Technology §2`):

1. **Header** — a search pill spanning the available width, then persistent
   `Paintbrush` **Draw** and `Settings` icon buttons, then `⋯` **More**. Draw and
   Settings remain visible without hover; each toggles its companion. The
   header is also the drag handle: pressing anywhere in it that is not a
   control moves the window. There is no title, wordmark or window control row.
2. **Resize layer** — eight invisible handles inset at the window's edges and
   corners, above the content and below the header's controls.
3. **Timeline** — the scrolling body. Items newest-first, grouped by section.
   A group header is a letterspaced name and a hairline; clicking it makes that
   section active. Each row is an item card (`Design System §4`) with exactly
   one active card, a ring around every selected card and one visible Copy
   button that strengthens on hover, focus, selection or activity.
4. **Composer** — pinned to the bottom. A `⊕` button *inside* the left of the
   field, the field itself, and above it a dictation indicator when recording.
5. **Toast and live region** — one transient visual line above the composer and
   one polite screen-reader live region. The visual line lasts 2.6 seconds;
   errors remain until resolved or explicitly dismissed, and neither stacks.

**States.**

| State | What the panel shows |
|---|---|
| Initial | Onboarding (§2) if it has never been completed, otherwise the timeline |
| Empty — first use | The face glyph, the build question, and the capture hint |
| Empty — no results | "No matches" and the query, with **no** capture hint |
| Populated | Sections in order, active section first |
| Offline | Capture, drawing, clipboard work and unexpired signed-lease use are unchanged; licence state shows `Offline licence` until revalidation succeeds |
| Restricted | Trial ended or the signed lease expired: the timeline stays legible; copy/export remain available; composer, capture, dictation, drawing, attachment and update commands are disabled; the persistent Settings icon opens the licence group |

**Copy.**

- Search field placeholder: `Search`
- Composer placeholder: `Add a note or a prompt` — with the active section's
  name appended in lowercase parentheses when one is active, as in
  `Add a note or a prompt (research)`
- First-use title: `What should we build?`, or `What should we build in <Section>?`
- First-use hint: `Select text anywhere, then tap ⇧ ⇧` — rendered with the two
  Shift keys as key caps
- No-results title: `No matches`
- No-results hint: `Nothing here matches "<query>".`
- Toasts: `Copied` · `Copied as list` · `Added <n> file(s)` ·
  `Cleared <n> completed` · `Exported` · `Export failed` · `Section created` ·
  `Couldn't copy.` · `Couldn't add <n> file(s).`
- Restricted trial: `Your 14-day trial has ended. Activate Bard to keep creating.`
- Restricted lease: `We couldn't verify this licence. Connect to the internet or activate a valid key.`

**Overflow menu** (`⋯`), in order: **Favorites only** (a toggle) ·
**Clear completed** · **Export to Markdown**.

**Composer menu** (`⊕`): **Attach**. Drawing is a persistent header
action, so it does not appear as a second route in this menu.

**Item context menu**, opened by right-click, `Shift+F10`, or the Menu key on
the active item: **Copy** · **Copy as list** (when more than one item is
selected) · separator · **Favorite** · **Edit** · **Add caption** (images only)
· separator · **Delete**. It uses the native-desktop menu treatment in
`Design System §4`, makes its item active, and restores focus to that item when
closed.

**Fails when.**

- *A copy fails.* The item's Copy affordance returns to rest, the error cue is
  suppressed when another error is already sounding, and the panel announces
  `Couldn't copy.`; Bard never substitutes `Copied` for a failed clipboard
  write.
- *A context menu would cross a screen edge.* It clamps to the work area and
  keeps the active item beneath the pointer or focus origin.
- *A Send to Bard file cannot be read.* Valid files still import. Each failed
  path is named in the panel's error state, and no partial file is registered.
- *The panel loses focus mid-edit.* An open editor commits; nothing is
  discarded by the window hiding.
- *A capture arrives while the panel is scrolled down.* The timeline does not
  jump. The new item is at the top where it will be found.
- *Search excludes the active section entirely.* The no-results state renders
  rather than an empty section header with nothing under it.
- *Licence revalidation fails while the signed lease remains valid.* Existing
  work continues; the panel does not turn a transport warning into a restricted
  state or block the current operation.

**Leaves to.** The Settings companion · the Drawing companion · the section
switcher · the desktop, on Escape or focus loss.

---

## §2 Onboarding

**Purpose.** Establish the product's value, the three core gestures and the
licence choice in one short pass, then stop.

**Entered from.** The panel, on any launch where onboarding has not been
completed. It replaces the panel's content rather than opening a window.

**Zones.** A centred mark, one title, one short paragraph and one primary
action. Four steps: welcome · local voice · keys · trial or licence. Progress
is a four-segment line, not a row of badges.

**States.** Welcome is static. The voice step carries not started · downloading
with progress and Cancel · ready · failed with Retry. The licence step carries
local trial available · key entry · activating · active · machine limit ·
service error. Skipping key entry starts the 14-day local trial and never
consumes a paid activation.

**Copy.**

- Step 1 title: `Capture. Dictate. Keep.`
- Step 1 body: `Bard keeps selected text, images and drawings in one keyboard-first panel, then turns speech into text on your PC.`
- Step 2 title: `Local voice`, naming the model it is about to fetch
- Step 2 body: `Audio capture and transcription stay on this computer.`
- Step 3 title: `Three gestures`, then: `Double-tap Left Shift to capture a selection, double-tap Right Shift to open Bard, and double-tap the same Ctrl to dictate. Hold the second Ctrl for push-to-talk.`
- Step 4 title: `Try Bard free`
- Step 4 body: `Every feature is available for 14 days. No account required.`
- Licence field label: `Licence key`
- Licence actions: `Activate` · `Start trial`
- Step 4 option: `Start Bard at login` / `Runs quietly in the tray`
- Activation success: `Bard is activated.`
- Machine limit: `All 3 activations are in use. Deactivate a machine, then try again.`
- Activation failure: `Couldn't activate Bard. Check your connection and try again.`

**Fails when.**

- *The model download fails.* The voice step offers Retry and Continue. Local
  capture and clipboard work remain usable, and dictation reports its own model
  state when first used.
- *Activation is offline or the key is rejected.* The entered key is not
  persisted, the local trial remains untouched, and the step offers Retry or
  Start trial.
- *All three machines are active.* The page offers **Manage** and Start trial;
  it never offers a fourth activation or silently releases another machine.
- *The trial record cannot be protected locally.* Bard does not start a paid
  session or pretend the trial is active; the licence step reports a local
  credential-store failure and offers a retry.

**Leaves to.** The panel, with onboarding recorded as completed and the chosen
local trial or signed paid state in force.

---

## §3 Settings

**Purpose.** Every preference bard has, one window, eight panes.

**Entered from.** The panel's persistent Settings icon, `Ctrl+,`, the tray's
**Settings**, or a restricted-state action. The companion is created at startup
and hidden; opening it docks it beside the panel, matches the panel height and
takes focus.

**Zones.** A frameless companion window (`Technology §2`): a compact title row
with **Settings** and a close icon, then a sidebar of eight items on the left
and the active pane on the right. The sidebar order is `General` · `Capture` ·
`Dictation` · `Models` · `Refine` · `Shortcuts` · `Data` · `About`.
Each pane is a title, a one-line description, then groups. A group is one card
with hairlines between its rows; explanatory notes sit above the card. At narrow
widths the sidebar becomes labelled icons with tooltips; pane content scrolls
rather than clipping.

### 3.1 General

`General` · `How Bard behaves when you are not looking at it.`

**Appearance** → Theme, a three-way segmented control: System · Light · Dark.
Hint: `System follows your Windows appearance setting.`
**Startup** → Start at login, a toggle. Hint: `Runs quietly in the tray.`
**Sounds** → Sounds, a master toggle; Volume, a 0–100 slider; and five compact
event toggles: Capture, Copy, Dictation start, Dictation stop, Error. The group
hint reads: `Short cues confirm actions and states. Every cue has a visual
equivalent.` Audio implementation is `Technology §1`; the palette is
`Design System §4`.

### 3.2 Capture

`Capture` · `Grabbing text, images and files from anywhere on the machine.`

**Shortcut** → the double-Shift capture toggle.
**Windows integration** → Send to Bard, a toggle. Hint: `Adds one Send to Bard
command to Explorer's file context menu.` Its secondary status reads `Ready`,
`Needs repair`, `Unavailable`, or `Off`; **Repair** is available only for
`Needs repair`. The installer registers the verb per user, and the application
verifies the exact command rather than trusting the stored preference.

### 3.3 Dictation

`Dictation` · `Speech to text on this machine, with optional post-processing.`

**Input** → Microphone, a select. Hint: `The system default is used unless you
pick one.` The pane states that audio and local transcription stay on the
device; if Refine is on, only the already-produced transcript
text is sent to OpenRouter after local transcription.

**Overlay position** → two independent segmented controls: Vertical (`Top` or
`Bottom`) and Horizontal (`Left`, `Middle`, or `Right`). They compose into six
valid positions. The setting is persisted immediately and applies to the next
visible overlay, including a live reposition when the overlay is already open.
Bard keeps a safe margin from the screen edge and clamps the pill on unusually
small displays.

### 3.4 Models

`Models` · `Choose what Bard uses for local transcription.`

The pane has three named groups, in the order a person encounters them:

1. **Bard models** — the three first-party catalogue roles: **Fast multilingual**
   (`whisper-small-Q6_K.gguf`), **Best English**
   (`parakeet-tdt-0.6b-v2-Q6_K.gguf`), and **Best multilingual**
   (`Qwen3-ASR-1.7B-Q6_K.gguf`). Each row shows the display name, role,
   description, hosted state, source repository, exact size, language count,
   parameter count, Q6_K quantization, relative `Quality` and `Speed` meters,
   and its current installation state. The catalogue record behind the row
   carries the exact conversion repository and revision, model-specific
   licence, byte size, SHA-256, and `miisodev/bard-models` release `models-v2`
   asset; those values are pinned, not user-editable.
2. **On this computer** — compatible files discovered in the shared Hugging
   Face cache. Each row shows the repository, file, size, source revision when
   available, and the engine compatibility result. A compatible external file
   can be checked and then used; **Forget** removes Bard's reference only and
   never deletes the external file.
3. **Advanced model import** — a **Hugging Face repository or resolve URL**
   field with placeholder `owner/repository or huggingface.co/owner/repository/resolve/main/model.gguf`,
   a **Check repository** action, and the compatible files found in the
   repository. The accepted inputs are an `owner/repository` identifier, a
   Hugging Face repository URL, or a direct Hugging Face `resolve/main/<file>`
   URL. A listed file offers **Install & use** through its compatibility gate;
   an arbitrary non-Hugging-Face URL is not an import source.

**First-party actions are explicit.** An absent file offers **Download & use**
and **Download only**. An installed inactive file offers **Use** and **Remove**.
An active file remains visibly active, and the native deletion boundary refuses
to remove it until another verified model is active. Download and install
complete full-file checksum and engine-load checks before activation can write
`model_id`. Removing a Bard-managed row opens the inline confirmation
`Remove <model> from Bard?` with `Cancel` and `Remove`.

**Local-file actions are explicit.** A discovered file can be **Check again**;
a compatible file can be **Use**; an external row can be **Forget**. If a
managed copy is represented in the local inventory, its action is **Remove**
and the same ownership boundary applies. An external cache file has no
destructive delete action. The native model manager owns path containment for
Bard-managed files and treats the shared cache as read-only external state.

**Advanced-import actions are explicit.** A listed file is initially marked
**Verify on install** when the repository listing has not yet proved engine
compatibility; **Install & use** is allowed, and the exact file is probed during
installation. A file that has passed the probe is labelled **Compatible**; an
unsupported file is labelled **Not supported**. A running transfer shows
received bytes, total bytes, percent, transfer speed and ETA when available,
**Cancel**, and **Retry** on failure. The import registers a Bard-owned file
only after the complete file passes its SHA-256 and the current
`transcribe-cpp` engine loads it.

**Meters appear only on Bard models.** Quality and speed are coarse relative
guidance within those three roles. A discovered or imported model has no
measured scores, so its row shows no bars and makes no cross-model quality
claim. The model pane has no guaranteed search or language-filter control;
discovery is list-based.

**Fails when.**

- *The model link cannot be inspected.* The repository row reports the input
  error and no file is enabled for installation.
- *No compatible file is found.* The advanced-import area reports that no
  compatible model files were found.
- *A Bard asset or import fails download, checksum verification, or engine
  loading.* The file is not activated, the error is shown in its row, and a
  Bard-managed transfer offers **Retry**. Bard never offers to delete an
  external cache file.

### 3.5 Shortcuts

`Shortcuts` · `Two taps of a modifier, in any app. Shift captures, Ctrl dictates.`

A **Gestures** group comes first, because the gestures are the defaults: rows
**Capture**, **Panel** and **Dictate**, each showing its keys as keycaps
(`Left Shift` `Left Shift`, `Right Shift` `Right Shift`, `Ctrl` `Ctrl`). They
cannot be rebound. A **Chords** group follows, noted `Optional. A chord takes
its key combination away from every other app.`: rows **Capture** and **Panel**,
each unassigned by default and showing **Record** until a chord is recorded,
then the chord as keycaps with a quiet clear control. A **Reset** row clears
every chord; the gestures always stay.

### 3.6 Data

`Data` · `Everything lives in one SQLite file on this machine.`

**Export** → the export folder, and `Export to Markdown`, hinted
`Writes bard-export.md to the folder above.`

**History** → two independent selects. **Keep clipboard items** and **Keep
dictations** each offer `1 day`, `7 days`, `30 days`, `90 days`, `180 days`, and
`Forever`; both default to `30 days`. The note reads: `Favorites, drawings and
items you create in Bard are kept until you delete them.` A **Prune now** action
runs the same owned-file-safe pass used by the automatic schedule. A successful
pass reports `History pruned.`; a failed pass leaves every item and file in
place and reports `Couldn't prune history. Try again.`

**Diagnostics** → `Latest crash report`, with the hint `Crash evidence is
written locally before Bard exits. This never includes audio, captures, or API
keys.` **Load report** reads the newest local report. With no report, the pane
shows `No crash report has been recorded on this machine.`; a read or decode
failure shows its error without blocking the rest of Data.

**Danger** → `Clear history`, hinted `Removes every captured item. This cannot
be undone.`, with a `Clear` action.

### 3.7 About

`About` · the product name and the line `Capture. Dictate. Keep.`, the version,
a **Licence** group, and a **Privacy** group.

**Licence** shows one of these states without exposing the key or lease:

- Trial: `Trial · <n> days left` and `Start trial` is replaced by `Activate`.
- Active: `Licensed · <n> activations of 3 used` and `Renews <date>`.
- Revalidation: `Checking licence…` while one request is in flight; the prior
  signed lease remains valid and the surface stays interactive.
- Offline with a valid lease: `Offline licence · <n> days left`.
- Restricted: the matching panel restricted message, plus `Activate` and
  `Manage`.
- Machine limit: `All 3 activations are in use. Deactivate a machine, then try
  again.`

The licence-key field is masked and appears only when no key is stored or when
**Replace key** is explicitly selected. **Activate** stores and activates;
**Manage** opens the provider's customer portal; **Deactivate** releases this
installation after an inline confirmation; **Replace key** submits a new key and
clears the old paid session only after the new activation succeeds. No action
reveals a stored credential through the webview.

- `Local by default` — `Audio capture, speech recognition, captures, and history stay on this computer. Bard has no account, sync, or telemetry.`
- `Refine, only if you turn it on` — `Bard then sends the finished transcript text — and nothing else — to OpenRouter. Audio, captures, history, and unrelated items are never sent, and the untouched local transcript is used whenever Refine is off or fails.`

**There is no credits list and no third-party licence roll.** bard ships no
code from either reference product, and a paid product does not open its About
pane with other people's names. The obligations it does carry are stated in
`Product & Business §8`.

### 3.8 Refine

`Refine` · `Optional cloud cleanup of finished transcripts. Off by default.`

The privacy note reads: `Audio and local transcription always stay on this
machine. When Refine is on, only the already-produced
transcript text is sent to OpenRouter.` A lower note adds: `Bard has no
account, sync, or telemetry.`

**OpenRouter** contains one off-by-default `Cloud post-processing` toggle, an
`OpenRouter API key` masked field, and one `OpenRouter model IDs` field. The
model field uses provider-prefixed IDs separated by commas, for example
`vendor/model`; the values are preserved exactly. Bard uses only the first ID
for a request and never selects a fallback automatically. There is no endpoint
field, provider selector, or prompt library. The API is always Bard's fixed
official HTTPS OpenRouter endpoint.

The key field sends a replacement only through an explicit save action. The
native service stores it through keyring v1 in Windows Credential Manager and
returns only whether a key is configured; the key is not returned in
application state or events. The stored model-ID setting preserves the
comma-separated list, while the first ID remains the only one sent.

The actions are **Save**, **Test connection**, and **Clear key**. **Test
connection** validates the stored key and model through OpenRouter metadata;
it sends no transcript. **Clear key** turns the stage off and removes the
credential through the native keyring boundary. The pane's status text reports
save, test, and failure outcomes without exposing a key or provider response
body.

**States.** Off · saving or testing · on · failed. The local transcript is
always the fallback for the next explicitly started dictation when the stage is
off, unconfigured, or unavailable.

**Fails when.**

- *The model ID is missing while enabling the stage.* `Enter the exact
  OpenRouter model ID before enabling post-processing.`
- *The API key is missing while enabling the stage.* `Add an OpenRouter API key
  before enabling post-processing.`
- *The key cannot be saved or cleared.* The existing credential is retained and
  the pane reports the credential-store failure without exposing the secret.
- *OpenRouter rejects the key, model, quota, or request.* Bard shows a safe
  warning and pastes the preserved local transcript unchanged.
- *The cloud response is rate-limited, malformed, empty, truncated, or
  policy-blocked.* Bard never pastes a partial cloud result; it uses the
  untouched local transcript.

**Settings-wide states.** Each pane reads its values from the store on open. A
pane whose values have not loaded renders its controls disabled rather than at a
default that would be written back on the first interaction.

**Settings-wide failures.**

- *A shortcut the person records is already held by another application.*
  Registration fails, the row says so, and the previous binding stays in force.
  A binding that is stored but not registered is the worst outcome, because
  settings then disagrees with the machine.
- *The export folder is gone.* Export fails with `Export failed` in the panel's
  toast rather than creating a folder the person did not ask for.
- *A retention pass cannot finish.* No database row or owned file is removed;
  the Data pane reports the failure and the next scheduled or manual pass can
  retry.
- *The Explorer verb registration does not match the stored state.* Capture
  shows `Needs repair`; Repair rewrites only Bard's own verb and verifies it
  again.
- *A licence request fails.* The current signed lease is preserved, the key
  remains masked, and the About pane distinguishes an offline temporary state
  from a service rejection or a restricted state.
- *A companion cannot obtain focus.* It stays visible beside the panel and the
  panel's title region announces `Settings` or `Drawing`; Bard does not report
  success by closing a hidden window.

**Leaves to.** Hidden, on close, with focus returned to the panel. The companion
is never destroyed — see
`Technology §2`.

---

## §4 The drawing canvas

**Purpose.** Make an image that lands in the timeline as a retained item.

**Entered from.** The panel's persistent Paintbrush **Draw** icon or
`Ctrl+Shift+D`. It opens the Drawing companion beside the panel at exactly the
panel's current height; the composer does not offer a duplicate Create Drawing
command.

**Zones.** A frameless companion window (`Technology §2`): a compact title row
with **Drawing** and a close icon, one compact toolbar, and the canvas. The
toolbar carries Select, Pen, Line, Arrow, Rectangle, Ellipse and Text; five
stroke colours; three widths; Undo, Redo, Delete, **Copy** and **Save**. Icon
buttons have one-word tooltips and shortcut hints. The canvas fills the
remaining companion area and is always white.

**States.** Empty canvas · drawing · committed · copy confirmation · save
error. The title row's dirty marker is one small dot, never a blocking modal.
**Save** writes a PNG into Bard's images directory, adds a `manual` image item,
clears the drawing model only after the item and file are durable, then hides
the companion. **Copy** puts the current PNG on the clipboard without adding an
item. Closing a dirty canvas opens one confirmation: `Discard this drawing?`
with `Cancel` and `Discard`; closing a clean canvas hides it immediately.

**Fails when.**

- *Committing an empty canvas* adds nothing and leaves the clean companion open
  so the person can keep working.
- *The image cannot be written* leaves the drawing intact and reports `Couldn't
  save drawing.`; no timeline item is created.
- *The clipboard rejects the image* leaves the drawing intact and reports
  `Couldn't copy.`.
- *The panel resizes or moves* recomputes companion height and side in the same
  frame; canvas contents and pointer coordinates remain aligned.

**Leaves to.** The panel, with a saved drawing at the top of the timeline or
focus restored to the Draw icon when closed empty or discarded.

---

## §5 The dictation overlay

**Purpose.** Say that Bard is listening, show bounded recording state, and offer
the one way out without taking focus.

**Entered from.** Dictation starting, by any route.

**Zones.** A frameless, transparent, never-focused window (`Technology §2`):
one borderless pill holding a small state glyph, the state word, a compact
level meter, and a `✕`. Listening uses transform-animated bars; Transcribing and
Finishing use a slow indeterminate sweep with no layout animation. The pill is
centred in the transparent stage with no clipped border or shadow. Reduced
motion replaces both with a static glyph and state word.

**It never takes focus.** Focus during dictation belongs to whatever the person
is typing into; an overlay that steals it would paste the transcript into
itself. The start cue finishes before the microphone opens and the stop cue
begins after it closes, as specified in `Technology §1`.

**States.**

| State | Shows |
|---|---|
| Listening | `Listening`, bars tracking the microphone |
| Transcribing | `Transcribing`, bars frozen |
| Refine | `Finishing`, bars frozen after local transcription has completed |
| Local fallback | The cloud warning is reported outside the overlay; the preserved local transcript is pasted |
| Hidden | Nothing. The window stays alive and empty between dictations |

**Copy.** `Listening` · `Transcribing` · `Finishing` · the `✕` is labelled
`Cancel`. The six placement choices are Top/Bottom combined with Left/Middle/
Right.

**Fails when.**

- *No microphone.* `No microphone found.` and the overlay never appears.
- *No model installed.* `Voice model not downloaded yet. Open Settings to
  install it.`
- *The active model file disappears after the initial check.* `Voice model not
  downloaded.` Bard reports the failure and does not substitute a different
  model without the owner's choice.
- *Nothing audible.* `Nothing heard.` The overlay closes and nothing is pasted.
  Pasting an empty transcript into a person's document is the failure this
  avoids.
- *Transcription fails.* The engine's own message, shown once.
- *Cloud post-processing fails or returns unusable text.* Bard emits a safe
  warning after local transcription, uses the preserved local transcript, and
  never pastes a partial cloud result. If the panel is hidden, the same warning
  is delivered through a native notification and the error cue; it is not left
  solely in a hidden toast.
- *An error occurs with the panel hidden.* A native notification names the
  problem and action once. Opening the panel exposes the same message in its
  live region and persistent error state.

**Leaves to.** Itself hidden, and the transcript to its destination — see
`Input & Commands §4`.

---

## §6 The section switcher

**Purpose.** Move between sections, and create one, without leaving the
keyboard.

**Entered from.** The panel's section shortcut, and a section group header.

**Zones.** A sheet over the panel: a scrim, a card, a filter field, and the
`Menu` list from `Design System §4`. `Inbox` is always first. Typing filters;
Arrow keys move; Enter selects; a query matching nothing offers to create it.
Focus returns to the panel with the chosen section active and the composer
placeholder updated.

**States.** Populated · filtered with no match · duplicate name selects the
existing section. The filter and menu remain enabled in every state; only the
creating row appears when the query has no exact section match.

**Copy.** Filter placeholder `Sections…` · the creating row reads
`Create section "<query>"` · an empty filtered list reads `No matching sections.`.

**Fails when.** *A name that already exists* selects the existing section
rather than creating a second one with the same name.

**Leaves to.** The panel, with the chosen section active and the composer
placeholder updated.

---

## §7 The shortcut sheet

**Purpose.** Show every current command and fixed gesture without hiding the
values behind a settings workflow.

**Entered from.** `Ctrl+/` in the focused panel, or **Shortcut guide** at the
bottom of Settings → Shortcuts. It opens as a sheet over its invoker and never
changes a binding.

**Zones.** A scrim, one compact card, a title, two groups—**Gestures** and
**Keys**—and a close icon. Fixed gestures use `Left Shift × 2`, `Right Shift ×
2`, and `Ctrl × 2`; ordinary rows show the current normalized key chord. A
single **Open Settings** action returns to Settings → Shortcuts.

**States.** Normal · copied row · recording unavailable. Copying a row uses the
same Copy action and `Copied` confirmation as every other shortcut. The sheet
has no loading state because it reads only the already-registered in-memory
map plus the three fixed gestures.

**Copy.** Title `Shortcuts` · group labels `Gestures` and `Keys` · close label
`Close` · footer action `Open Settings`.

**Fails when.** *A stored normal binding no longer registers.* The sheet shows
that row as `Unavailable`, names `Open Settings`, and never displays the stored
value as if it worked. *A fixed gesture conflicts with the operating system.*
The fixed row remains visible and the activation failure uses the same safe
toast as any rejected gesture.

**Leaves to.** Its invoker, with focus restored to the exact control that opened
the sheet.

---

*Surfaces · version 1.2*
