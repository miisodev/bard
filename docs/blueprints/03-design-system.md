# bard — Design System

## §0 Using This Blueprint

**Scope.** Everything true of every surface: philosophy, tokens, type, space,
shape, motion, themes, the component set, accessibility, and the patterns that
are refused. Anything true of one surface belongs to `Surfaces`.

**Deliverables.** `src/styles/tokens.css`, `src/styles.css`, and the component
set in `src/components/` and `src/settings/ui.tsx`.

**Cross-reference convention.** References to other documents carry the document
name, as in `Surfaces §2`. A bare §5 is a section of this document.

**Anchor policy.** The system font is named and linked; every token value is
bard's own and stated in full.

**The tell that a rule is in the wrong document:** it appears in three surface
entries. Rules appearing three times belong here.

---

## §1 Philosophy

**Copper's language, expressed with Windows primitives.** bard follows Copper's
visual design because its users should not have to relearn it, and renders it
with the system's own font and conventions rather than transplanting macOS ones.
A Mac design faithfully copied onto Windows looks like a port, which is the one
thing this product cannot afford to look like.

**The light palette is measured, not invented.** Copper's panel is `#e5e6e9`,
its cards `#f8f8f8`, its fields `#f3f4f7`. bard's values sit within one unit of
those.

**Content over chrome.** The panel carries no branding, title bar or window
controls, and cards carry no timestamps or multi-button action rows. Its header
does carry two persistent line-icon actions—Draw and Settings—beside the search
field and overflow, and every item carries one Copy affordance. *The cost of the
wrong choice:* a panel that advertises itself competes with what it holds, while
controls that exist only in a context menu make the common action slower than
the product's keyboard-first claim.

**The empty space is the design.** A card is a checkbox, its content and one
copy control. The copy control is always present in the accessibility tree and
at low visual prominence; card hover, keyboard focus, selection or active state
raises its contrast. Actions beyond Copy and Done remain in the context menu,
where a desktop user expects to find them without putting a toolbar on every
row.

**Small does not mean cryptic.** Persistent state may use a compact badge or
status dot, but colour is never the only carrier. Buttons use familiar Lucide
line icons, unfamiliar actions carry one-word tooltips, and menu labels use a
single verb or noun. Decorative badges, nested cards and controls added only to
fill visual balance are absent.

---

## §2 Tokens

Every value the system uses lives in `src/styles/tokens.css`. **Nothing
downstream invents a colour, a size, a radius, a shadow or a duration.** If a
value is needed and missing, it is added there first.

### Type

The system face is **Segoe UI Variable**, the Windows 11 system font and the
native counterpart to Copper's SF Pro. Its optical sizes are separate families
rather than weights: Text below roughly 16px, Display above, Small for captions.

```css
--font-text:    "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
--font-display: "Segoe UI Variable Display", "Segoe UI Variable Text", …;
--font-small:   "Segoe UI Variable Small", "Segoe UI Variable Text", …;
--font-mono:    "Cascadia Code", "Cascadia Mono", ui-monospace, "Consolas", monospace;

--fs-caption: 11px;   /* section labels, meta */
--fs-sm:      12px;   /* secondary UI text */
--fs-base:    14px;   /* card body — what most of the app is read at */
--fs-md:      15px;   /* composer, emphasised body */
--fs-lg:      17px;   /* sheet titles */
--fs-xl:      22px;   /* onboarding, empty states */

--fw-normal: 400;  --fw-medium: 500;  --fw-semibold: 600;  --fw-bold: 700;
--lh-tight: 1.25;  --lh-base: 1.45;   --lh-loose: 1.6;
--tr-caption: 0.07em;  --tr-tight: -0.01em;  --tr-normal: 0;
```

**`font-synthesis: style` is required and is not a detail.** Segoe UI Variable
ships no italic face. With synthesis disabled, `*emphasis*` renders upright,
silently — invisible to a typecheck and to a colour measurement, catchable only
by reading rendered text. Weight synthesis stays off, because the family has a
real weight axis.

### Space

A four-based scale. Spacing is never a literal.

```css
--sp-1: 2px;   --sp-2: 4px;   --sp-3: 6px;   --sp-4: 8px;
--sp-5: 10px;  --sp-6: 12px;  --sp-7: 16px;  --sp-8: 20px;
--sp-9: 24px;  --sp-10: 32px; --sp-11: 40px;
```

### Shape

```css
--r-panel: 16px;  --r-card: 12px;  --r-control: 10px;  --r-sm: 8px;  --r-pill: 999px;
--hairline: 1px;
```

### Motion

**Measured, never bouncy.** Nothing overshoots and nothing runs long enough to
be waited on.

```css
--ease-out:    cubic-bezier(0.32, 0.72, 0, 1);
--ease-in-out: cubic-bezier(0.65, 0, 0.35, 1);
--t-instant: 90ms;  --t-fast: 130ms;  --t-base: 180ms;  --t-slow: 260ms;
```

Under `prefers-reduced-motion: reduce` every duration is zero. Transitions
remain declared; they simply complete immediately.

### Layout

`--panel-w` tracks the panel window's width, which `Technology §2` owns. The
two are changed together; a token that disagrees with the window it fills
produces a horizontal scrollbar nobody can find the cause of.

```css
--panel-w: 380px;      --panel-pad: var(--sp-6);
--card-pad-y: var(--sp-5);  --card-pad-x: var(--sp-6);
--card-gap: var(--sp-3);    --checkbox-size: 18px;
--icon-control: 32px;      --icon-glyph: 17px;
```

---

## §3 Themes

**Three: System, Light, Dark.** System resolves against the OS appearance and
keeps following it. The resolved theme is an attribute on the document root.

**Every window follows the stored theme and keeps following it.** A window reads
the theme when it opens and listens for changes; a theme changed in settings
reaches the panel, the canvas and the overlay. *The cost of the wrong choice:* a
theme that reaches only the window it was changed in reads as a broken product
within one click.

### Light

```css
--canvas: #e6e7ea;        /* the panel chrome */
--surface: #f9f9fb;       /* cards on it */
--surface-hover: #ffffff;
--surface-2: #f3f4f7;     /* fields */
--surface-3: #ffffff;     /* popovers, sheets */
--text: #1d1d1f;  --text-2: #5f6065;  --text-3: #66676d;
--text-on-accent: #ffffff;
--border: rgba(0,0,0,.07);  --border-strong: rgba(0,0,0,.14);
--rule: rgba(0,0,0,.08);
--accent: #2f6fed;  --accent-hover: #2560d8;
--accent-soft: rgba(47,111,237,.12);  --accent-text: #2560d8;
--danger: #e5484d;  --danger-soft: rgba(229,72,77,.12);  --success: #2c9e5b;
--el-card:  0 1px 2px rgba(0,0,0,.05), 0 0 0 .5px rgba(0,0,0,.04);
--el-pop:   0 8px 32px rgba(0,0,0,.16), 0 0 0 .5px rgba(0,0,0,.06);
--el-panel: 0 16px 56px rgba(0,0,0,.24), 0 2px 8px rgba(0,0,0,.1);
--scrim: rgba(0,0,0,.26);  --scrollbar: rgba(0,0,0,.22);
--focus-ring: 0 0 0 3px var(--accent-soft), 0 0 0 1px var(--accent);
```

### Dark

```css
--canvas: #1c1c1f;  --surface: #26262a;  --surface-hover: #2e2e33;
--surface-2: #2e2e33;  --surface-3: #303036;
--text: #f5f5f7;  --text-2: #a1a1a2;  --text-3: #8c8d94;
--border: rgba(255,255,255,.08);  --border-strong: rgba(255,255,255,.16);
--rule: rgba(255,255,255,.1);
--accent: #3f73d8;  --accent-hover: #315fb8;
--accent-soft: rgba(63,115,216,.24);  --accent-text: #9ab8f7;
--danger: #ff6369;  --success: #43c07c;
```

**`--el-panel` belongs to the companion surface, not the panel.** The panel
draws no shadow of its own: it fills its window exactly, so a shadow has no
transparent margin to fall into and is clipped square at the window edge,
reading as a smudge worst at the corner where the offsets meet. Settings and
Drawing reserve the transparent margin their shadow needs. See
`Technology §2`.

---

## §4 Components

Every component entry states purpose and when not to use it, anatomy outside-in,
variants, states, and accessibility.

### Item card

**Purpose.** One captured item. **Not** for a toolbar of unrelated actions; Copy
is the one direct command and the context menu owns the rest.

**Anatomy.** Card → checkbox (18px, flex-none, `margin-top: 2px`) · main column
→ body text, then thumbnail row · Copy button aligned to the first line's
optical centre. The button has a 32 × 32 CSS-pixel hit area and a 17px Lucide
`Copy` glyph; the hit area may extend into the card padding without changing
the text wrap.

**The checkbox sits on the first text line's optical centre, not the card's.**
On a three-line card, centring it on the box looks like a mistake.

**States.** Default · hover (`--surface-hover`, Copy at full contrast) · selected
but not active (a 1px accent ring, no fill) · active (a 2px accent ring plus
`--accent-soft` wash and `aria-current="true"`) · done (accent-filled circle,
white check, struck-through and dimmed body, thumbnails at 50%). A selected and
active card combines the active wash and ring; a filled card is never used for
selection because it competes with the done state.

The Copy button is never removed from layout or the accessibility tree. At rest
it uses `--text-3`; hover, focus-visible, selected and active states raise it to
`--text`. Pointer, keyboard and context-menu activation all emit the same copy
command and leave the active card active.

**Accessibility.** The timeline is a named list; each card is a list item and
the active card carries `aria-current="true"`. Selected state is included in
each item's accessible description, and selection changes are announced in the
panel's polite live region. The checkbox is a `role="checkbox"` button with
`aria-checked` and a label that states the action it performs. Copy has the
accessible name `Copy <item preview>` and a visible focus ring.

### Section header

A letterspaced uppercase label and a hairline reaching the right edge. No icon,
no count. **Clicking it makes that section active.**

### Header actions

Three 32px controls share the panel header's trailing group: a persistent
Lucide `Paintbrush` labelled **Draw**, a persistent Lucide `Settings` labelled
**Settings**, and an overflow button labelled **More**. Draw and Settings never
appear only on hover. Their pressed state is a low-contrast `--accent-soft`
background plus `aria-expanded`; More has no persistent state.

### Menu

**Purpose.** The header overflow, composer attach menu, item context menu and
section picker use one keyboard-complete menu component. The context variant
follows the familiar Windows/7-Zip desktop pattern: pointer-anchored, grouped,
icon-plus-label rows, separators only between action groups, and a right-aligned
keyboard hint. The popover variants anchor to their invoking control.

**Anatomy.** Scrim for popover variants → menu surface → optional group label
→ menu items → optional separator → optional trailing hint. Context menus omit
the scrim and clamp their rectangle to the work area. An item is disabled rather
than removed when its precondition is temporarily false; unavailable discovery
results use explanatory copy rather than a dead command.

`ArrowUp` and `ArrowDown` move, `Home` and `End` jump, printable letters jump to
a matching item, `Enter` and `Space` activate, and `Escape` closes. Focus returns
to the exact invoker. The menu uses `role="menu"` and `role="menuitem"`; check
state uses `menuitemcheckbox`. Opening the item context menu makes that item
active before the menu takes focus.

**Escape closes it, on a capture-phase listener.** Without capture, the panel's
own Escape handling hides the whole window first.

### Toggle · Segmented · Select · Button · TextField · Slider

The settings primitives, in `src/settings/ui.tsx`. **Each takes its spacing,
colour and type from the tokens and exposes no `style` prop.** A pane needing an
escape hatch means the primitive is wrong.

`Button` variants: default, primary, danger. `Segmented` is a `radiogroup`;
`Toggle` is a `switch` with `aria-checked`; `Slider` exposes the native range
keyboard model and an accessible value text.

### Meter

A 0–100 reading as a bar. **Never a number** — a number implies a precision
these scores do not have.

### Settings row and group

A group is one card with hairlines between rows, not a card per row. *The cost
of the wrong choice:* a card each turns a settings page into a list of floating
slabs. A group's explanatory note sits **above** the card; inside it, a note
reads as a setting.

### Audio cues

The palette contains five semantic cues and no per-control sound:

| Event | Character | Visual counterpart |
|---|---|---|
| `Capture` | One soft rounded pop | Toast or new active item |
| `Copy` | One short neutral click | `Copied` toast or button confirmation |
| `DictationStart` | Quiet rising two-note tone | Overlay enters Listening |
| `DictationStop` | Quiet falling two-note tone | Overlay leaves Listening |
| `Error` | Low double pulse, never a harsh alarm | Error text or restricted treatment |

Each asset is under 350ms, uses the default Windows output, and has an
independent enable toggle plus one master volume from 0–100. Hover, menu
opening and every button press make no sound. Cue files are bundled and
versioned with the design system; user content never enters playback or naming.

---

## §5 States

Nine states are designed for every surface: initial, loading, empty, populated,
success, error, disabled, restricted, offline.

**"Empty" is five states, not one.**

| Kind | Occurs when | Action offered | Illustration |
|---|---|---|---|
| First use | Nothing has ever been captured | The creating action | Yes — the only kind that earns one |
| No results | A search excluded everything | Clear the search. **Never the creating action** | No |
| Cleared | The user emptied it | The creating action | No |
| Restricted | Trial or licence term ended | Purchase; content stays visible behind | No |
| **Error-empty** | A read failed; contents unknown | Retry | No |

**Error-empty is the one that causes harm.** A failed read rendering "Nothing
captured yet" tells someone their work is gone. A surface that cannot
distinguish *there is nothing* from *I could not find out* renders the error.

---

## §6 Accessibility

**Target: WCAG 2.2 AA**, at [w3.org/TR/WCAG22](https://www.w3.org/TR/WCAG22/).

- **Contrast** 4.5:1 for body text, 3:1 for large text and UI boundaries, in
  both themes and every state. Verification rasterises rendered output rather
  than comparing computed colour strings; disabled and placeholder colours are
  held to the same target or carry a non-text alternative.
- **Every action has a key.** The mouse is optional throughout, including item
  Copy, the desktop-style context menu, section selection, companion windows and
  licence activation.
- **Focus is always visible** via `--focus-ring`. `outline: none` without a
  replacement ring is an error, and the CSS audit rejects it.
- **Roles, names and state.** Every control has an accessible name; custom
  checkbox, switch, radio, menu and slider semantics match their native
  equivalents. Focus order follows visual order and never lands on a hidden or
  disabled control.
- **Announcements.** The panel has one polite live region for copy, capture,
  selection, retention and licence outcomes. The never-focused dictation
  overlay mirrors Listening, Transcribing, Finishing, cancellation and errors
  into the panel region or a native notification when the panel is hidden.
  Errors are announced once and remain visible until dismissed or resolved.
- **Reduced motion** zeroes every duration, and no information is carried by
  motion alone.

**Keyboard and screen-reader verification is release evidence, not a later
polish pass.** Every interactive surface is exercised without a pointer and
with a screen reader before the production gate in `docs/PARITY.md` can pass.

---

## §7 Anti-patterns

These are natural reaches that the system refuses on principle.

| Refused | Why |
|---|---|
| A title bar with minimise and close on the panel | The panel is dismissed from the keyboard. Window controls make it a window to manage |
| A wordmark in the panel | The panel is content. Branding belongs on the product page and installer |
| Timestamps on cards | Noise on a list that is visible in full |
| A multi-button action row on every card | Copy is the only direct item command; the rest stay in a keyboard-complete context menu |
| Copy available only from a context menu | The common recovery action becomes slower than keyboard-first use claims |
| Hover-only header controls | Settings and Draw disappear from both pointer discovery and touch-style exploration |
| A translucent theme | A translucent panel cannot hold contrast over arbitrary desktop content |
| A placeholder that teaches syntax | The composer describes intent. Mechanics are learned once, from the shortcut sheet |
| Empty score bars for an unrated model | Claims it was measured and found wanting |
| `style` props on settings primitives | The escape hatch becomes the pattern; that is how a design system dies one inline style at a time |

---

## §8 Enforcement

**`scripts/css-audit.mjs` runs in the build** and fails it on duplicate
selectors and undefined custom properties.

**Duplicate selectors are the failure mode this system is most exposed to**,
because CSS is last-wins and a stale duplicate silently overrides the rule that
replaced it — with no error and no visual difference large enough to notice. It
is the reason the audit exists.

Unused classes are reported and do not fail the build: classes composed at
runtime look unused to a static scan. The list is reviewed beside a rendered
pass because the opposite defect—a class with no rule at all—renders unstyled
without producing a duplicate or undefined-token error.

---

*Design System · version 1.2*
