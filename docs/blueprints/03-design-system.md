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

**Content over chrome.** The panel carries no branding, no title bar, no window
controls and no per-card action row. Every one of those was present once and
each was removed. *The cost of the wrong choice:* a panel that advertises itself
competes with what it holds, and what it holds is the product.

**The empty space is the design.** A card is a checkbox and its text. Resist
adding affordances to it; everything a card can do is reachable from the
keyboard or the context menu.

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
--text: #1d1d1f;  --text-2: #6e6e73;  --text-3: #a1a1a6;
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
--text: #f5f5f7;  --text-2: #98989d;  --text-3: #6e6e73;
--border: rgba(255,255,255,.08);  --border-strong: rgba(255,255,255,.16);
--rule: rgba(255,255,255,.1);
--accent: #5b8def;  --accent-hover: #74a0f5;
--accent-soft: rgba(91,141,239,.18);  --accent-text: #9ab8f7;
--danger: #ff6369;  --success: #43c07c;
```

**`--el-panel` is declared and unused by the panel.** The panel draws no shadow
of its own: it fills its window exactly, so a shadow has no transparent margin
to fall into and is clipped square at the window edge, reading as a smudge worst
at the corner where the offsets meet. See `Technology §2`.

---

## §4 Components

Every component entry states purpose and when not to use it, anatomy outside-in,
variants, states, and accessibility.

### Item card

**Purpose.** One captured item. **Not** for anything needing its own actions —
those live in the context menu.

**Anatomy.** Card → checkbox (18px, flex-none, `margin-top: 2px`) · main column
→ body text, then thumbnail row.

**The checkbox sits on the first text line's optical centre, not the card's.**
On a three-line card, centring it on the box looks like a mistake.

**States.** Default · hover (`--surface-hover`) · selected (a **ring**, never a
fill — a filled card competes with the done state, and the two are read
together constantly) · done (accent-filled circle, white check, struck-through
and dimmed body, thumbnails at 50%).

**Accessibility.** The checkbox is a `role="checkbox"` button with `aria-checked`
and a label that states the action it performs.

### Section header

A letterspaced uppercase label and a hairline reaching the right edge. No icon,
no count. **Clicking it makes that section active.**

### Popover menu

**Purpose.** The header's overflow and the composer's attach menu — the same
component, positioned differently.

**Anatomy.** Scrim (catches the outside click, so nothing underneath knows the
menu exists) → menu → items, optional separators, optional trailing hint.

**Escape closes it, on a capture-phase listener.** Without capture, the panel's
own Escape handling hides the whole window first.

### Toggle · Segmented · Select · Button · TextField

The settings primitives, in `src/settings/ui.tsx`. **Each takes its spacing,
colour and type from the tokens and exposes no `style` prop.** A pane needing an
escape hatch means the primitive is wrong.

`Button` variants: default, primary, danger. `Segmented` is a `radiogroup`;
`Toggle` is a `switch` with `aria-checked`.

### Meter

A 0–100 reading as a bar. **Never a number** — a number implies a precision
these scores do not have.

### Settings row and group

A group is one card with hairlines between rows, not a card per row. *The cost
of the wrong choice:* a card each turns a settings page into a list of floating
slabs. A group's explanatory note sits **above** the card; inside it, a note
reads as a setting.

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
| Restricted | Trial expired | Purchase; content stays visible behind | No |
| **Error-empty** | A read failed; contents unknown | Retry | No |

**Error-empty is the one that causes harm.** A failed read rendering "Nothing
captured yet" tells someone their work is gone. A surface that cannot
distinguish *there is nothing* from *I could not find out* renders the error.

---

## §6 Accessibility

**Target: WCAG 2.2 AA**, at [w3.org/TR/WCAG22](https://www.w3.org/TR/WCAG22/).

- **Contrast** 4.5:1 for body text, 3:1 for large text and UI boundaries, in
  both themes. Verified by rasterising rendered output, not by comparing
  computed colour strings — a translucent surface over an unknown backdrop has
  no computable contrast, which is one reason no such surface exists.
- **Every action has a key.** The mouse is optional throughout.
- **Focus is always visible** via `--focus-ring`. `outline: none` without a
  replacement ring is an error the CSS audit is extended to catch in Phase 2.
- **Roles and labels.** Every control that is a `<button>` standing in for a
  checkbox, switch or radio carries the matching role and state.
- **Reduced motion** zeroes every duration.

**Screen-reader support is Phase 2**, and is named rather than assumed: the
panel's live regions, the dictation overlay's announcements and the timeline's
list semantics are unspecified until then.

---

## §7 Anti-patterns

Each of these was present in the product and removed. They are recorded because
each is a natural thing to reach for.

| Refused | Why |
|---|---|
| A title bar with minimise and close | The panel is dismissed from the keyboard. Window controls make it a window to manage |
| A wordmark in the panel | The panel is content. Branding belongs on the product page |
| Timestamps on cards | Noise on a list that is visible in full |
| A per-card action row | Actions are keyboard and context menu |
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
runtime look unused to a static scan. The list is read rather than obeyed, and
it has already surfaced the opposite defect — a class with no rule at all,
rendering unstyled.

---

*Design System · version 1.0*
