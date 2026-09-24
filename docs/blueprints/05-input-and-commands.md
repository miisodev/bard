# bard — Input & Commands

## §0 Using This Blueprint

**Scope.** The command surface of a keyboard-first application: the global
shortcuts and the gestures, the capture mechanism, dictation's triggers and
where a transcript goes, the clipboard monitor, the panel's own keys, and the
tray. It owns what each input *does*; the surfaces those inputs reach are
specified in `Surfaces`.

**Cross-reference convention.** References to other documents carry the
document name, as in `Surfaces §5`. A bare §3 is a section of this document.

**Notation.** Keys are written as they read on a Windows keyboard.
`⇧ ⇧` means two taps of the same Shift key within the tap window; `Ctrl Ctrl`
means two taps of the same Control key.

**The governing rule.** bard is reached without the mouse and used without the
mouse. Every action in the product has a key; the tray and the menus are
alternatives, never the only route.

---

## §1 Global shortcuts

Bard's global shortcuts are fixed. They are reached with a finger already
resting on a modifier, from inside any application, and there is no key editor
or persisted custom binding.

| Gesture | Action |
|---|---|
| Double-tap **Left Shift** | Capture the current selection |
| Double-tap **Right Shift**, then `1` / `2` / `3` within the slot window | Paste the first / second / third timeline item into the focused app |
| Double-tap **Right Shift** without a number | Show or hide the panel after the slot window expires |
| Double-tap **Left Alt** | Open or put away Draw as a single window |
| Double-tap **Right Alt** | Open or put away Settings as a single window |
| Double-tap the same **Control** key | Start or stop dictation |

The item slots use the current timeline order and do not open the panel. A
missing item is a no-op; the failed paste is not converted into a new capture.
The panel-key and gesture entry points for Settings and Draw also open one
companion window, while tray and header buttons retain their existing companion
behavior.

**Why they are fixed.** They are reached with a finger already resting on the
key, from inside any application, without taking a chord away from another
application. There is no custom binding editor.

### The gestures' own rules

A tap counts only when the key goes down and comes back up **within 500
milliseconds with no other key pressed in between**, and the second press must
begin **within 400 milliseconds** of the first. Right Shift waits that same
400-millisecond slot window for `1`, `2`, or `3`; if no supported number arrives,
the panel toggle runs. The Control pair waits a further 50 milliseconds after
the second press so a following normal shortcut such as `Ctrl+Space` invalidates
it before dictation starts. Auto-repeat, already-held foreign keys, mouse
buttons, and Bard's own synthetic copy/paste chords never arm a gesture.

**Without the dirty rule the gestures fire constantly.** Shift is held down
during ordinary capitalisation, and a person typing two capitals in a sentence
would otherwise be capturing their own selection mid-word.

Left and right are tracked separately: the two Shift gestures never collide,
Left Alt and Right Alt never cross-trigger, and Ctrl-left followed by Ctrl-right
is not Ctrl+Ctrl. The second Control release carries its hold duration to the
dictation lifecycle: a short release leaves recording locked; a release at or
beyond 300 milliseconds stops it.

---

## §2 Capture

**Capture takes what is selected in another application, without that
application knowing it happened.**

The mechanism, in order:

1. Read and hold whatever is on the clipboard.
2. Write a sentinel value — a zero-width-bracketed marker that no human types.
3. Synthesise the copy chord.
4. Poll for up to roughly 700 milliseconds. The first read that is **neither
   the sentinel nor empty** is the selection, and it is captured.
5. If the sentinel is still there when polling runs out, nothing was selected:
   the original clipboard is restored and the capture reports failure.

**The sentinel is what makes the failure case distinguishable.** Without it, a
capture with nothing selected reads back the clipboard's previous contents and
captures them — the person gets a duplicate of something they copied earlier
and no indication that anything went wrong.

**Captures do not overlap.** A capture already running makes a second request a
no-op rather than queueing, because two of these interleaved would restore each
other's clipboard.

**Capture clipboard** is the separate, simpler action: take what is on the
clipboard now, with no synthesis and no sentinel. It is on the tray menu.

### Send to Bard

**Explorer contributes one command and no application modes.** **Send to Bard**
hands the selected regular files to the attachment path, shows the panel and
makes the imported result inspectable. A warm instance receives the handoff;
a cold instance starts once, initializes storage and consumes the same
handoff. The command never recurses into folders, launches a separate elevated
process, or accepts a shell command from the selected path.

Files enter the same bounded importer as **Attach**. Duplicate paths in
one handoff collapse, ordinary attachments become `manual` retained items, and
supported images can stand alone. Every rejected path has its own safe failure;
one unreadable file does not discard valid files. The registration mechanism is
`Technology §7`; the exact result states are `Surfaces §1`.

---

## §3 The clipboard monitor

bard watches the clipboard and captures what it sees.

**It polls the sequence number**, not the contents — a counter the operating
system increments on every clipboard write. Reading the clipboard itself on a
timer would fight every other application that wants it, and repeated reads of
a large image are expensive for nothing.

Three things are skipped, and each is skipped for its own reason:

| Skipped | Why |
|---|---|
| A sequence number bard itself last produced | Otherwise every copy from the panel is re-captured, and the item doubles on each use |
| Clipboard content marked as excluded from history by its source | Password managers set it. It is the whole reason the marker exists, and honouring it is not optional in a product that keeps what it sees |
| Content identical to the last captured item | A copy of the same thing twice is one item |

**Clipboard origin and lifespan are explicit.** Selection capture, explicit
clipboard capture and monitor-created rows receive the `clipboard` origin.
Dictation rows receive `dictation`; composer notes, drawings and deliberate file
attachments receive `manual`. Clipboard and dictation lifespans are independent
and default to 30 days, with the allowed values, favourite exemption, schedule
and owned-file deletion rules owned by `Product & Business §5`.

**A failed retention pass is a failed pass, not a partial cleanup.** Database
rows and Bard-owned image or attachment files commit or roll back as one unit;
external cache files are never candidates. The next startup, daily tick or
manual **Prune now** retries the complete pass.

---

## §4 Dictation

**Hold to dictate; tap to toggle.** The same-key `Ctrl Ctrl` gesture does
both, resolved by how long the second press was held:

- The first tap arms the gesture. The second press starts dictation after a
  50 ms activation grace, unless another key invalidates the chord.
- Held for **300 milliseconds or more** — release stops the recording and
  transcribes. This is the ordinary push-to-talk case.
- Released **under 300 milliseconds** — recording continues in a locked state.
  A later complete `Ctrl Ctrl` gesture stops it. This is what makes a long
  dictation possible without holding a key for two minutes.

The gesture is same-side: left Ctrl twice or right Ctrl twice. It is fixed and
is not stored as a normal OS shortcut. A tray or settings action can still
start a take, and a release token prevents a late gesture release from stopping
a take owned by another source.

**Escape cancels while recording**, and it is observed by the shared raw-input
coordinator rather than shadowing Escape anywhere else for the rest of the
session.

**Silence stops it.** Roughly 1.6 seconds of detected silence ends a recording
on its own. Voice activity uses a 60-millisecond onset and a 600-millisecond
hangover, so a pause mid-sentence does not end the take. Input buffers and
recording length are bounded; a resampler or device failure ends the take with
an error instead of accumulating audio indefinitely.

### Where a transcript goes

| When | Destination |
|---|---|
| The panel is visible **and** focused | The clipboard, and the timeline as a `dictation`-origin item governed by the dictation lifespan |
| Anything else has focus | The clipboard, then the paste chord into that application; no timeline copy is created |

**bard pastes into whatever the person was typing into**, which is the entire
point of dictation in another application. The clipboard is written first in
both cases, so a paste that does not land leaves the transcript recoverable
rather than lost. A failed clipboard write cancels the paste, leaves no partial
timeline item, and reports `Couldn't copy.` through the panel or native
notification when the panel is hidden.

**The paste chord is a synthesised keystroke and is treated as one.** It waits
for the clipboard write to settle before pressing, and releases every modifier
it pressed. A chord that leaves Control held turns the next thing the person
types into shortcuts in their own document.

**An empty transcript is never pasted.** It reports `Nothing heard.` — see
`Surfaces §5`.

---

## §5 Panel keys

Active whenever the panel has focus.

| Key | Action |
|---|---|
| `↑` `↓` | Move the active item. With nothing active, `↓` takes the first item and `↑` the last |
| `Shift+↑` `Shift+↓` | Extend the selected range from the active anchor |
| `Enter` or `Ctrl+C` | Copy the active item, or the whole selection when it contains more than one |
| `Ctrl+Shift+M` | Copy the selected items as a Markdown list |
| `Shift+F10` or Menu | Open the desktop-style context menu for the active item |
| `Space` | Toggle done for the whole selection, using the active item's state |
| `Delete` / `Backspace` | Delete the selection |
| `Ctrl+K` | Open the section switcher |
| `Ctrl+/` | Open the shortcut sheet |
| `Escape` | Close a menu, then clear the search, then the selection, then hide the panel |
| Typing | Goes to the composer, which holds focus by default |

**Active and selected are different states.** Exactly one item is active. It is
set by the first arrow movement, a pointer activation, or opening that item's
context menu. Extending with Shift selects a range but does not silently move
the active anchor; a plain arrow collapses the range to the newly active item.
Copying, deleting and opening a menu do not change the active item. Search
clears the active item, then activates the first match when one exists.

**Space toggles the whole selection off the active item's state**, not each
item's own. Toggling each independently splits a mixed selection into two
halves and the second press does not put it back.

**Escape unwinds one layer at a time**, and the order is not arbitrary: a
person pressing Escape to clear a search does not expect the window to vanish.
The open menu or sheet consumes it first, on a capture-phase listener — see
`Design System §4`.

**Enter in the composer submits; `Shift+Enter` makes a new line.** A note is
usually one line and a paragraph is the exception.

---

## §6 The tray

bard is always running, and the tray is where it lives when the panel is
hidden.

| Item | Action |
|---|---|
| **Show Bard** | Shows and focuses the panel |
| **Draw** | Toggles the Drawing companion |
| **Settings** | Toggles the Settings companion |
| **Capture selection** | §2 |
| **Capture clipboard** | §2 |
| **Dictate** | Starts dictation |
| **Start at login** | A checked item, mirroring the setting |
| **Quit Bard** | Marks a clean exit, then exits |

**A left click toggles the panel; the menu opens on right click.** The common
action is the one-click one.

**Tooltip:** `Bard — double-tap Right Shift to open`. It is the only place the
gesture is taught outside onboarding, and it is where a person looks when they
have forgotten how to summon the window.

**Quit marks a clean exit before exiting.** Without the marker the guardian
treats the exit as a crash and relaunches the application the person just quit
— see `Technology §9`.

---

## §7 Focus and dismissal

**The panel hides when it loses focus**, which is what makes it a panel rather
than a window to manage.

Three rules keep that from firing when it should not:

1. **A grace period.** Focus loss is acted on after roughly 140 milliseconds,
   not immediately. Opening a menu or a sheet moves focus for a frame.
2. **A settling period after showing.** For roughly 700 milliseconds after the
   panel appears it does not auto-hide. A panel summoned by a global shortcut
   can receive a focus-change event that arrives before it is properly in
   front, and would hide itself the instant it opened.
3. **Focus is checked by process, not by window.** If the foreground window
   belongs to bard, the panel stays. A window-level check is unreliable
   *during* a focus transition, which is exactly when it is consulted.

**Opening Settings or Drawing does not hide the panel**, because both belong to
bard. A companion takes focus while it is open, but its window and webview
belong to the same process, so foreground movement between the panel and either
companion never triggers auto-hide. Closing or hiding the companion returns
focus to the panel. Activating the other companion header icon closes the first
one before opening the second.

---

*Input & Commands · version 1.2*
