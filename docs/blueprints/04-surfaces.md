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
- Step 3 title: `You're set`, then: `Tap ⇧ ⇧ to capture a selection, hold
  Ctrl Space to dictate, and open the panel with Ctrl Shift Space.` — each key
  rendered as a key cap
- Step 3 option: `Start Bard at login` / `Runs quietly in the tray`

**Fails when.** *The model download fails.* Onboarding completes anyway. A
person who cannot reach Hugging Face on first run still has a working capture
and clipboard product, and dictation reports its own missing model when it is
first used. Blocking here would make a network failure look like a broken
purchase.

**Leaves to.** The panel, with onboarding recorded as completed.

---

## §3 Settings

**Purpose.** Every preference bard has, one window, seven panes.

**Entered from.** The panel's overflow menu. The window is created at startup
and hidden; opening it shows and focuses it.

**Zones.** A native window (`Technology §2`): a sidebar of seven items on the
left, the active pane on the right. Each pane is a title, a one-line description, then
groups. A group is one card with hairlines between its rows; explanatory notes
sit above the card.

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

`Dictation` · `Speech to text, entirely on this machine.`

**Input** → Microphone, a select. Hint: `The system default is used unless you
pick one.` A note states that audio never leaves the machine.

### 3.4 Models

`Models` · `Transcription runs offline. Pick the model it runs.`

Three groups, in the order a person tries them:

1. **Transcription models** — the curated catalogue. Each row: name,
   description, size, an accuracy meter and a speed meter, and one action —
   Download, Use, or In use.
2. **Already on this machine** — models found in the shared Hugging Face cache.
   Same row shape, **no meters**, and the action is always Use.
3. **From Hugging Face** — a repository field and a listing of the
   transcription files it contains, each installable.

**Meters appear only on catalogue entries.** A discovered or pasted model has
no scores, and empty bars would claim it had been measured and found wanting.

### 3.5 Shortcuts

`Shortcuts` · `Global keys, active whatever you are using.`

Three rebindable rows — **Capture selection**, **Toggle panel**, **Dictate** —
each recorded by pressing the combination. A **Fixed** group lists the two
double-tap gestures, which cannot be rebound. A **Reset shortcuts** row puts
every binding back: `Puts every binding back to its default.`

### 3.6 Data

`Data` · `Everything lives in one SQLite file on this machine.`

**Export** → the export folder, and `Export to Markdown`, hinted
`Writes bard-export.md to the folder above.`
**Danger** → `Clear history`, hinted `Removes every captured item. This cannot
be undone.`, behind a confirmation.

### 3.7 About

`About` · the product name and the line `Capture. Dictate. Keep.`, the version,
and a **Privacy** group stating that nothing captured or spoken leaves the
machine.

**There is no credits list and no third-party licence roll.** bard ships no
code from either reference product, and a paid product does not open its About
pane with other people's names. The obligations it does carry are stated in
`Product & Business §8`.

**States.** Each pane reads its values from the store on open. A pane whose
values have not loaded renders its controls disabled rather than at a default
that would be written back on the first interaction.

**Fails when.**

- *A shortcut the person records is already held by another application.*
  Registration fails, the row says so, and the previous binding stays in force.
  A binding that is stored but not registered is the worst outcome, because
  settings then disagrees with the machine.
- *A model download is interrupted.* The partial file is not registered.
  Resuming uses a range request; a checksum mismatch discards and re-fetches.
- *A repository has no transcription files.* `No transcription models (.gguf or
  ggml-*.bin) in this repository.`
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
| Listening | `Listening…`, bars tracking the microphone |
| Transcribing | `Transcribing…`, bars frozen |
| Hidden | Nothing. The window stays alive and empty between dictations |

**Copy.** `Listening…` · `Transcribing…` · the `✕` is labelled `Cancel`.

**Fails when.**

- *No microphone.* `No microphone found.` and the overlay never appears.
- *No model installed.* `Voice model not downloaded yet. Open Settings to
  install it.`
- *Nothing audible.* `Nothing heard.` The overlay closes and nothing is pasted.
  Pasting an empty transcript into a person's document is the failure this
  avoids.
- *Transcription fails.* The engine's own message, shown once.

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

## §7 The ask result

**Purpose.** Show the answer to a prompt typed into the composer, attached to
the prompt that produced it.

**Entered from.** The composer, when the text is a prompt rather than a note.

**Zones.** A card above the composer: the prompt, the answer, then a row of
actions — copy, keep, dismiss.

**States.** Busy, with the answer area showing progress · answered · dismissed.

**Copy.** The actions are the only fixed strings; everything else is content.

**Fails when.** *The answer cannot be produced.* The card shows the failure in
place and keeps the prompt, so nothing typed is lost. **A prompt is never
silently turned into a note.**

**Leaves to.** The timeline, if kept. Otherwise nothing is written.

---

*Surfaces · version 1.0*
