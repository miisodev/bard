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
`⇧ ⇧` means two taps of the same Shift key within the tap window.

**The governing rule.** bard is reached without the mouse and used without the
mouse. Every action in the product has a key; the tray and the menus are
alternatives, never the only route.

---

## §1 Global shortcuts

Three bindings, active whatever application has focus.

| Action | Default | Rebindable |
|---|---|---|
| Capture selection | `Ctrl+Shift+C` | Yes |
| Toggle panel | `Ctrl+Shift+Space` | Yes |
| Dictate | `Ctrl+Space` | Yes |

**Two gestures are fixed and cannot be rebound:**

| Gesture | Action |
|---|---|
| Double-tap **Left Shift** | Capture the current selection |
| Double-tap **Right Shift** | Show or hide the panel |

**Why they are fixed.** They are reached with a finger already resting on the
key, from inside any application, without a chord. Making them configurable
would invite a binding that cannot be typed, and there is no second gesture of
the same quality to fall back to.

### The gesture's own rules

A tap counts only when the Shift key goes down and comes back up **within 500
milliseconds with no other key pressed in between**, and the second press must
begin **within 400 milliseconds** of the first. Any other key pressed while
Shift is held marks the sequence dirty and discards it.

**Without the dirty rule the gesture fires constantly.** Shift is held down
during ordinary capitalisation, and a person typing two capitals in a sentence
would otherwise be capturing their own selection mid-word.

Left and right are tracked separately: the two gestures never collide, and a
tap on one side does not arm the other.

### Binding and registration

A binding is stored as a string and parsed into a modifier set and a key code.
**A binding is only in force once registration with the operating system
succeeds.** On startup every stored binding is validated; one that no longer
parses is replaced by its default rather than left dead. *The cost of the wrong
choice:* a settings row showing a shortcut that was never registered is worse
than a reset, because the product then disagrees with the machine about what is
bound and the person has no way to see it.

**Registration can fail because another application holds the combination.** The
failure is reported where the binding was made — see `Surfaces §3.5`.

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

**History is bounded.** Older items are pruned past the configured limit,
which defaults to 500.

---

## §4 Dictation

**Hold to dictate; tap to toggle.** The same key does both, resolved by how
long it was held:

- Held for **more than 300 milliseconds** — release stops the recording and
  transcribes. This is the ordinary case: hold, speak, let go.
- Tapped and released **under 300 milliseconds** — recording continues.
  Pressing again stops it. This is what makes a long dictation possible without
  holding a key for two minutes.

**One threshold serves both**, so nothing has to be configured and nothing has
to be learned. A person who holds it gets push-to-talk; a person who taps gets
a toggle; neither is told which one they used.

**Escape cancels while recording**, and it is registered only for that window,
so it does not shadow Escape anywhere else for the rest of the session.

**Silence stops it.** Roughly 1.6 seconds of detected silence ends a recording
on its own. Voice activity uses a 60-millisecond onset and a 600-millisecond
hangover, so a pause mid-sentence does not end the take.

### Where a transcript goes

| When | Destination |
|---|---|
| The panel is visible **and** focused | The clipboard, and the timeline as a captured item |
| Anything else has focus | The clipboard, then the paste chord into that application |

**bard pastes into whatever the person was typing into**, which is the entire
point of dictation in another application. The clipboard is written first in
both cases, so a paste that does not land leaves the transcript recoverable
rather than lost.

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
| `↑` `↓` | Move the selection. With nothing selected, `↓` takes the first item and `↑` the last |
| `Shift+↑` `Shift+↓` | Extend the selection from the anchor |
| `Enter` | Copy the selection |
| `Space` | Toggle done |
| `Delete` / `Backspace` | Delete the selection |
| `Ctrl+K` | Open the section switcher |
| `Escape` | Clear the search, then the selection, then hide the panel |
| Typing | Goes to the composer, which holds focus by default |

**Space toggles the whole selection off the lead item's state**, not each
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

**Opening settings or the canvas does not hide the panel**, because both belong
to bard.

---

*Input & Commands · version 1.0*
