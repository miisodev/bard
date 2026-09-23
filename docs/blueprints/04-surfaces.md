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
capture, which shows the panel if it is hidden.

**Zones**, top to bottom, inside the frameless panel window (`Technology §2`):

1. **Header** — a search pill spanning the width, then one `⋯` button. The
   header is the drag handle: pressing anywhere in it that is not a control
   moves the window. There is no title, no wordmark and no window controls.
2. **Resize layer** — eight invisible handles inset at the window's edges and
   corners, above the content and below the header's controls.
3. **Timeline** — the scrolling body. Items newest-first, grouped by section.
   A group header is a letterspaced name and a hairline; clicking it makes that
   section active. Each row is an item card (`Design System §4`).
4. **Composer** — pinned to the bottom. A `⊕` button *inside* the left of the
   field, the field itself, and above it a dictation indicator when recording.
5. **Toast** — a transient line above the composer, 2.6 seconds, never
   dismissible and never stacked.

**States.**

| State | What the panel shows |
|---|---|
| Initial | Onboarding (§2) if it has never been completed, otherwise the timeline |
| Empty — first use | The face glyph, the build question, and the capture hint |
| Empty — no results | "No matches" and the query, with **no** capture hint |
| Populated | Sections in order, active section first |
| Offline | Unchanged. The panel has no network dependency |
| Restricted | Trial expired: the timeline stays legible and the composer is disabled |

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
  `Cleared <n> completed` · `Exported` · `Export failed` · `Section created`

**Overflow menu** (`⋯`), in order: **Favorites only** (a toggle) ·
**Clear completed** · **Export to Markdown** · **Settings**.

**Composer menu** (`⊕`): **Attach Files…** · **Create Drawing…**.

**Item context menu**, on right-click: **Copy** · **Copy as list** (when more
than one item is selected) · **Favorite** · **Edit** · **Add caption** (images
only) · **Delete**.

**Fails when.**

- *A copy fails.* The toast is the only signal the copy happened, so a failed
  copy shows no toast and nothing else. The clipboard write returns a result
  and a failure raises `Export failed`-style copy rather than silence.
- *The panel loses focus mid-edit.* An open editor commits; nothing is
  discarded by the window hiding.
- *A capture arrives while the panel is scrolled down.* The timeline does not
  jump. The new item is at the top where it will be found.
- *Search excludes the active section entirely.* The no-results state renders
  rather than an empty section header with nothing under it.

**Leaves to.** Settings · the drawing canvas · the section switcher · the
desktop, on Escape or focus loss.

---

## §2 Onboarding

**Purpose.** Get a new installation to the point where the three keys work,
and stop.

**Entered from.** The panel, on any launch where onboarding has not been
completed. It replaces the panel's content rather than opening a window.

**Zones.** A centred mark, a title, one paragraph, and one action. Three steps,
never more: welcome · the voice model · the keys.

**States.** The voice-model step carries its own: not started · downloading,
with a progress meter · ready · failed, with a retry.

**Copy.**

- Step 1 title: `Bard`
- Step 2 title: `Offline voice`, naming the model it is about to fetch
- Step 3 title: `You're set`, then: `Tap ⇧ ⇧ to capture a selection, tap the same
  Ctrl twice to dictate (hold the second press for push-to-talk), and open the
  panel with Ctrl Shift Space.` — each key rendered as a key cap
- Step 3 option: `Start Bard at login` / `Runs quietly in the tray`

**Fails when.** *The model download fails.* Onboarding completes anyway. A
person who cannot reach Bard's model host on first run still has a working
capture and clipboard product, and dictation reports its own missing model when
it is first used. Blocking here would make a network failure look like a broken
purchase.

**Leaves to.** The panel, with onboarding recorded as completed.

---

## §3 Settings

**Purpose.** Every preference bard has, one window, eight panes.

**Entered from.** The panel's overflow menu. The window is created at startup
and hidden; opening it shows and focuses it.

**Zones.** A native window (`Technology §2`): a sidebar of eight items on the
left, the active pane on the right. The sidebar order is `General` · `Capture` ·
`Dictation` · `Models` · `Post-processing` · `Shortcuts` · `Data` · `About`.
Each pane is a title, a one-line description, then groups. A group is one card
with hairlines between its rows; explanatory notes sit above the card.

### 3.1 General

`General` · `How bard behaves when you are not looking at it.`

**Appearance** → Theme, a three-way segmented control: System · Light · Dark.
Hint: `System follows your Windows appearance setting.`
**Startup** → Start at login, a toggle. Hint: `Runs quietly in the tray.`

### 3.2 Capture

`Capture` · `Grabbing text and images from anywhere on the machine.`

**Shortcut** → the double-Shift capture toggle.
**Feedback** → Capture sound. Hint: `A soft pop when something is captured.`

### 3.3 Dictation

`Dictation` · `Speech to text on this machine, with optional post-processing.`

**Input** → Microphone, a select. Hint: `The system default is used unless you
pick one.` The pane states that audio and local transcription stay on the
device; if Post-processing is enabled, only the already-produced transcript
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

`Shortcuts` · `Global keys, active whatever you are using.`

Two rebindable rows — **Capture selection** and **Toggle panel** — are recorded
by pressing the combination. A **Fixed gestures** group lists same-key
`Left Shift × 2`, `Right Shift × 2`, and `Ctrl × 2`; the Control gesture starts
or stops dictation and cannot be rebound. A **Reset shortcuts** row puts every
normal binding back.

### 3.6 Data

`Data` · `Everything lives in one SQLite file on this machine.`

**Export** → the export folder, and `Export to Markdown`, hinted
`Writes bard-export.md to the folder above.`

**Diagnostics** → `Latest crash report`, with the hint `Crash evidence is
written locally before Bard exits. This never includes audio, captures, or API
keys.` **Load report** reads the newest local report. With no report, the pane
shows `No crash report has been recorded on this machine.`; a read or decode
failure shows its error without blocking the rest of Data.

**Danger** → `Clear history`, hinted `Removes every captured item. This cannot
be undone.`, with a `Clear` action.

### 3.7 About

`About` · the product name and the line `Capture. Dictate. Keep.`, the version,
and a **Privacy** group.

- `Local by default` — `Audio capture, speech recognition, captures, and history stay on this computer. Bard has no account, sync, or telemetry.`
- `Optional cloud post-processing` — `If you explicitly enable Post-processing, Bard sends only the finished transcript text to OpenRouter. Audio, captures, history, and unrelated items are not sent. Bard falls back to the untouched local transcript whenever cloud processing is off or fails.`

**There is no credits list and no third-party licence roll.** bard ships no
code from either reference product, and a paid product does not open its About
pane with other people's names. The obligations it does carry are stated in
`Product & Business §8`.

### 3.8 Post-processing

`Post-processing` · `Optional text cleanup after local transcription. Off by
default.`

The privacy note reads: `Audio and local transcription always stay on this
machine. When cloud post-processing is enabled, only the already-produced
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

**Leaves to.** Hidden, on close. The window is never destroyed — see
`Technology §2`.

---

## §4 The drawing canvas

**Purpose.** Make an image that lands in the timeline as a captured item.

**Entered from.** The composer's `⊕` → **Create Drawing…**.

**Zones.** A native window (`Technology §2`): a single toolbar and the canvas. The
toolbar carries the tools, the stroke colour and width, undo, and one
commit action.

**States.** Empty canvas · drawing · committed. Committing writes a PNG into
the images directory and adds the item; the window hides.

**Fails when.** *Committing an empty canvas* adds nothing and closes.
*The image cannot be written* leaves the window open with the drawing intact
rather than closing on a failure and losing the work.

**Leaves to.** The panel, with the drawing at the top of the timeline.

---

## §5 The dictation overlay

**Purpose.** Say that bard is listening, and offer the one way out.

**Entered from.** Dictation starting, by any route.

**Zones.** A frameless, transparent, never-focused window (`Technology §2`):
one pill holding a dot, the state word, a twelve-bar level meter, and a `✕`.

**It never takes focus.** Focus during dictation belongs to whatever the person
is typing into; an overlay that steals it would paste the transcript into
itself.

**States.**

| State | Shows |
|---|---|
| Listening | `Listening`, bars tracking the microphone |
| Transcribing | `Transcribing`, bars frozen |
| Post-processing | `Finishing`, bars frozen after local transcription has completed |
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
  never pastes a partial cloud result.

**Leaves to.** Itself hidden, and the transcript to its destination — see
`Input & Commands §4`.

---

## §6 The section switcher

**Purpose.** Move between sections, and create one, without leaving the
keyboard.

**Entered from.** The panel's section shortcut, and a section group header.

**Zones.** A sheet over the panel: a scrim, a card, a filter field, and the
list. `Inbox` is always first. Typing filters; a query matching nothing offers
to create it.

**Copy.** Filter placeholder `Sections…` · the creating row reads
`Create section "<query>"`.

**Fails when.** *A name that already exists* selects the existing section
rather than creating a second one with the same name.

**Leaves to.** The panel, with the chosen section active and the composer
placeholder updated.

---

*Surfaces · version 1.1*
