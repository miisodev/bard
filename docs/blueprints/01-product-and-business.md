# bard — Product & Business

## §0 Using This Blueprint

**Scope.** What bard is, who buys it, what it does, what it stores, what it
costs and what it owes. This document owns every behaviour of the product and
the schema that behaviour runs on. It carries no visual decisions and no
per-window layout — those are `Design System` and `Surfaces`.

**Deliverables.** A shipping Windows application, a licence and activation
path, a purchase page, and the legal position that lets all three exist.

**Path conventions.** As `Index §0`.

**Cross-reference convention.** References to other documents carry the
document name, as in `Technology §3`. A bare §3 is a section of this document.

**Status key.** Phase 1 is the shipping product. Phase 2 completes parity with
the reference products. Phase 3 is beyond them. Markers describe scope, never
progress.

**Anchor policy.** Model licences, platform requirements and store policies are
named and linked, never transcribed.

---

## §1 What bard is

bard is a native Windows application that merges two things a person doing
AI-assisted work needs constantly and otherwise gets from two places: a place
to put the fragments worth keeping, and a way to speak instead of type.

It is **one keyboard-first panel**, always a shortcut away, that holds captured
text, images and drawings — and transcribes speech entirely on the machine,
against open-weights models the owner chooses.

**The product's claim is cohesion, not accumulation.** A clipboard manager and
a dictation tool running side by side are two utilities. bard is one surface
where dictated text lands in the same timeline as captured text, where the
model that transcribes is chosen in the same settings window that governs
capture, and where one shortcut map covers both halves without collision.

### The three tests

Every decision in this library answers to these, in order.

**The Copper test.** A long-time heavy user of shadcn's Copper picks up bard
and uses it with no caveats and no friction. Nothing they know transfers
wrongly; nothing they reach for is missing or renamed.

**The Handy test.** A long-time heavy user of Handy picks up bard and uses it
with no caveats and no friction. The model picker behaves as they expect, the
dictation shortcut behaves as they expect, and the settings they rely on exist.

**The standard above both.** Would Apple ship this into their product
ecosystem, and would they present it at WWDC. This is the test that rejects
work which passes the other two and is still not good enough: an unread
placeholder, an unexamined empty state, a window that has never been looked at.

### What bard is not

- **Not a note-taking application.** Dictation targets Wispr Flow's capability
  minus its notetaking. bard captures and transcribes; it does not organise
  long-form documents.
- **Not a cloud product.** No account, no sync, no telemetry, no server. Every
  capture and every recording stays on the machine. This is a feature and a
  constraint: it rules out server-side transcription, cross-device history, and
  any recovery of data from a lost machine.
- **Not a Copper clone.** bard follows Copper's interaction design because that
  design is right and its users should not have to relearn it. It is an
  independent product with its own name, its own dictation half, and no
  affiliation with or endorsement by shadcn.

---

## §2 The market and the reference products

bard sits between three products, and takes something different from each.

| Product | What it is | What bard takes | Where bard differs |
|---|---|---|---|
| **Copper** by shadcn | A Mac capture app: clipboard, scratchpad and to-do list purpose-built for AI-assisted work | The interaction design, the panel's shape, the keyboard-first discipline — **this is the target** | Windows-native rather than Mac; adds dictation |
| **Handy** | An open-source dictation app with a curated local-model catalogue | The shape of the model story: curated recommendations, one-click acquisition, no configuration sprawl | Adds the capture half; paid rather than free |
| **Wispr Flow** | A commercial AI dictation product | The capability bar — what good dictation feels like | Fully offline on open weights, rather than cloud; no notetaking |

**Copper is the target and `cooper/` is only a means.** The `cooper/`
repository in the workspace is an open-source recreation of Copper, read as a
reference implementation. Where the two disagree, Copper wins. The Copper
reference material is the demo captures in `~/Downloads/Cooper/` and
[shadcn.com/copper](https://shadcn.com/copper) — behaviour is taken from
watching those rather than inferred from the recreation's source.

### Target user

Someone doing AI-assisted work on Windows — writing prompts, collecting
answers, moving fragments between a browser, a terminal and a chat. They
already use a clipboard manager or wish they did, they type more than they want
to, and they will not tolerate a tool that sends their work to a server.

**Two constraints follow from who they are.** They are keyboard-first, so every
action has a key and the mouse is optional. And they are not machine-learning
practitioners — the model picker must let them choose well without knowing what
a mel spectrogram is.

---

## §3 Feature behaviour — capture

### The panel

A frameless always-on-top window, summoned and dismissed from the keyboard,
docked to the edge of the screen. It holds a search field, the timeline of
captured items grouped into sections, and a composer.

**It dismisses itself when it stops being the thing in use.** Losing foreground
to another application hides it; foreground moving to another bard window does
not. The timings and the checks that keep this from misfiring are in
`Input & Commands §7`. *The cost of the wrong choice:* a panel that stays up
becomes another window to manage, which is what it exists to avoid.

**Opening bard opens the panel.** Launching the application shows the window.
The single exception is a launch by Windows at login, which stays in the tray —
the autostart entry carries a flag, and that flag is the only way to tell "the
machine booted" from "someone opened this". *The cost of the wrong choice:* an
application that launches to nothing reads as a failure to start.

### Items

An item is a piece of text, an image, or a drawing. Every item carries a
**done** state, rendered as a hollow circle that fills and strikes the text
through. Done is the primary interaction on an item, as in Copper — not a
favourite, not a tag.

Text items render inline markdown: `**bold**`, `*italic*`, `~~strikethrough~~`
and `` `code` ``. **Underscore emphasis is deliberately unsupported**, because
captured text is mostly code and prose about code, where `snake_case` and
`__dunder__` are far commoner than _emphasis_ and supporting it would mangle
more than it formatted.

Captured text is stored and displayed exactly as the clipboard supplied it.
Where a source application HTML-encodes its newlines, bard shows the encoded
text. *The cost of the wrong choice:* decoding entities would corrupt anyone
copying HTML source, which is a normal thing for this user to do.

### Sections

Items belong to a section, and one section is active. Typing `# Name` in the
composer creates a section and makes it active. Typing `## Name` creates one and
clears the board — the new section becomes the working area and the rest
compress upward.

### Images and drawings

Images arrive by paste, by drag from another application, or from the composer's
attach menu. They are stored in bard's own folder and shown as thumbnails.
**Clicking a thumbnail copies the image back to the clipboard.**

A drawing is made in its own window, from the composer's attach menu, and
arrives in the timeline as an image item. The canvas is always white whatever
the theme, because a drawing is made to be pasted elsewhere and one drawn on a
dark surface arrives unreadable.

### Export

`Export to Markdown` writes a checklist-style file to a chosen folder,
defaulting to Documents. **Phase 2:** export to an Obsidian vault, one file per
section.

---

## §4 Feature behaviour — dictation

Holding the dictation shortcut records; releasing transcribes. The text is
pasted into whatever application has focus, or lands in the timeline when the
panel is open.

**Transcription runs entirely on the machine.** No audio and no text leaves it.
The model is chosen by the owner from a curated catalogue, from models already
present on the machine, or from any Hugging Face repository by name.

**A model is registered only once it has actually loaded.** A repository that
looks right but cannot run — an architecture the engine does not know, or a
conversion in a layout it cannot read — is refused at install time rather than
installed and left broken. *The cost of the wrong choice:* a model that fails
at first use fails during dictation, which is the worst moment to discover it.

The model unloads after five minutes idle and reloads on demand.

**Phase 2** completes the dictation settings surface: voice activity detection
and its backend, filler-word removal and its vocabulary, custom words, paste
method and delay, auto-submit, append-trailing-space, translate-to-English,
language selection, microphone muting during recording, output device, and
hardware acceleration selection. Each is a row in `Surfaces §3` when it lands.

**Phase 3** adds post-processing through a chosen language model provider,
which is the one capability in this half that is not offline and is therefore
off by default and explicitly opted into.

---

## §5 Data

Everything bard holds lives in one SQLite file in the application's own data
directory, beside a folder of images and a folder of models.

**The database is opened before any window exists.** Windows declared in the
application manifest are created before application setup runs, so anything
reacting to a window event can reach the database first. *The cost of the wrong
choice:* a settings read from a focus event before the database is open takes
the whole application down at launch, silently, because the release profile
aborts on panic.

```sql
CREATE TABLE items (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  section_id  INTEGER REFERENCES sections(id) ON DELETE SET NULL,
  content     TEXT    NOT NULL,   -- for kind='image', an absolute file path
  kind        TEXT    NOT NULL DEFAULT 'text',   -- 'text' | 'image'
  image_path  TEXT,
  favorite    INTEGER NOT NULL DEFAULT 0,
  done        INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_items_created ON items(created_at DESC);

CREATE TABLE sections (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT    NOT NULL,
  created_at INTEGER NOT NULL
);

-- Attachments are images bound to a text item, as distinct from image items,
-- which stand alone in the timeline.
CREATE TABLE attachments (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  path       TEXT    NOT NULL,
  position   INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_attachments_item ON attachments(item_id);

-- Models installed from Hugging Face rather than the catalogue. They carry no
-- curation, which is why the settings surface shows them without scores.
CREATE TABLE custom_models (
  id         TEXT PRIMARY KEY,
  repo       TEXT    NOT NULL,
  file       TEXT    NOT NULL,
  size_mb    INTEGER NOT NULL DEFAULT 0,
  local_path TEXT    NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

**Migrations are additive `ALTER TABLE` statements whose errors are ignored.** A
duplicate-column error is the expected result on every launch after the first.
*The cost of the wrong choice:* a version-numbered migration system is a second
source of truth about the schema, and the first time it disagrees with the
database it fails at startup on a user's machine rather than in a test.

**Settings read before the database is open return nothing, and writes are
dropped.** Before the database exists there genuinely are no settings, and
saying so is correct where panicking is not.

---

## §6 Pricing and licensing

**bard costs $30, once.** One purchase, no subscription, no seat count, no
expiry. Updates within the major version are included.

**Activation is offline.** The purchase issues a licence key; the application
validates it locally and never contacts a server again. *The cost of the wrong
choice:* a product that promises nothing leaves the machine cannot then phone
home to check a licence — the contradiction is visible to exactly the user this
product is for.

**A key activates up to three machines.** A person with a desktop and a laptop
is the normal case and should not have to think about it; three is generous
enough that the limit is never met honestly, and low enough to be worth having.
Deactivation is by reinstalling elsewhere — there is no server to revoke
against, which is the price of offline validation and is accepted.

**The trial is the product, time-limited.** Every capability is available for
14 days. *The cost of the wrong choice:* a feature-limited trial of a product
whose value is cohesion demonstrates the opposite of its argument.

---

## §7 Distribution

**miiso.dev is the channel.** The product page sells the licence and serves the
installer directly.

**The installer is signed.** An unsigned Windows binary triggers SmartScreen,
and a paid product that opens with a security warning has lost the argument
before it starts. Code signing is a Phase 1 requirement, not a polish item.

**Updates are checked in-app and applied by the user.** The application checks a
release feed, surfaces an available version, and downloads a signed installer on
request. It never updates silently.

**The Microsoft Store is Phase 3.** It reaches people who will not download an
installer, and its packaging and review requirements are a body of work that
does not block shipping. Requirements are at
[Microsoft Store policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies).

### Crash reporting

**Phase 1 requires a panic hook that writes a log before the process dies.** The
release profile aborts on panic and its symbols are stripped, so an unhandled
panic today exits with a status code and nothing else — a crash report from a
customer would be unusable. *The cost of the wrong choice:* the one class of
defect that cannot be reproduced on the developer's machine is exactly the one
that produces no evidence.

---

## §8 Legal position

**bard ships no code from either reference product.** `cooper/` is Apache-2.0
and `Handy/` is MIT; both are read as references. Where an approach was
learned from one of them, bard's implementation is its own. Attribution
obligations attach to code, so none is owed — and both licences' obligations
return the moment anything is vendored, a data file included.

**No credit list appears in the product.** A paid application does not carry a
list of what it learned from in its settings.

**Trademark position.** bard is an independent product, not affiliated with or
endorsed by shadcn, and does not use the Copper name, mark or branding. The
name, the icon and the product page are bard's own.

**Model licences are the model's, not bard's.** Each catalogue entry records the
upstream repository and revision it came from; the licence is the upstream's and
is linked from the entry. bard redistributes no model weights — it downloads
them from their canonical home on the user's machine, on the user's request.

**Model hosting is bard's own.** Whisper models are fetched from
[ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp), pinned to
a revision, which is their canonical upstream. Architectures whose GGUF
conversions upstream does not publish are converted and hosted under bard's own
organisation rather than taken from another project's infrastructure. *The cost
of the wrong choice:* serving a paid product's downloads from a free project's
bandwidth is not a licence question and is still not acceptable.

---

## §9 The product page

One page on miiso.dev. It carries what the product does, what it costs, the
system requirement, a purchase action and a download.

**It answers four questions in this order**, because that is the order a visitor
asks them: what is this, what does it look like, what does it cost, how do I get
it. Everything else — the offline claim, the model story, the shortcut map — is
supporting material below that fold.

**The offline claim is the headline, not a footnote.** It is the one thing no
competitor in this category can say, and the one thing this buyer cares most
about.

---

## §10 Metrics

bard collects no telemetry, so every product metric comes from the sales channel
rather than the application.

| Metric | Source | What it answers |
|---|---|---|
| Licences sold | Payment processor | Whether the product sells |
| Refund rate | Payment processor | Whether it is what buyers expected |
| Page-to-purchase conversion | Site analytics on miiso.dev | Whether the page argues well |
| Download-to-purchase | Site analytics and licence issuance | Whether the trial converts |

**There is no in-product funnel and there will not be one.** Knowing which
features a user touches would require watching them, and the product's central
promise forbids it.

---

*Product & Business · version 1.0*
