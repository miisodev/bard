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

**Anchor policy.** Model licences, provider requirements, platform requirements
and store policies are named and linked, never transcribed.

---

## §1 What bard is

bard is a native Windows application that merges two things a person doing
AI-assisted work needs constantly and otherwise gets from two places: a place
to put the fragments worth keeping, and a way to speak instead of type.

It is **one keyboard-first panel**, always a shortcut away, that holds captured
text, images and drawings and transcribes speech on the machine with the three
curated GGUF roles defined in `Technology §4` or another compatible local model.
Audio capture and local transcription remain on-device. When the person
explicitly enables Post-processing, only the already-produced transcript text
may be sent to the fixed OpenRouter endpoint through the first model ID they
entered; audio and unrelated local data remain on the machine.

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

**The production gate is all-or-nothing.** A release is eligible only when the
installed product passes all three tests across licensing, security, privacy,
keyboard operation, accessibility, visual QA, performance, failure recovery,
distribution and support. A feature does not inherit a waiver from the rest of
the product being finished. The evidence and current gate state live only in
`docs/PARITY.md`.

### What bard is not

- **Not a note-taking application.** Dictation targets Wispr Flow's capability
  minus its notetaking. bard captures and transcribes; it does not organise
  long-form documents.
- **Not a Bard-operated cloud product.** There is no Bard account, sync,
  telemetry, or data-processing backend. Captures and recordings stay on the
  machine, and the local transcript is produced there. When cloud
  post-processing is enabled, a copy of the text from the dictation that
  triggered it is sent to the fixed OpenRouter endpoint using the first model
  ID stored for that stage; audio, clipboard contents, history, images, and
  other captured items do not. The default rules out server-side transcription
  and cross-device history; enabling text post-processing does not change the
  local recording or transcription path.
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
| **Wispr Flow** | A commercial AI dictation product | The capability bar — what good dictation feels like | Local transcription on open weights, with optional text-only cleanup after transcription; no notetaking |

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
to, expect capture and audio to remain local, and require any cloud processing
of dictated text to be an explicit choice.

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

Every item also exposes **Copy** as one compact icon in its own anatomy. The
button, `Enter`, `Ctrl+C`, and the context-menu command are the same operation;
none depends on a hidden hover target. Exactly one item is **active** at a
time. Pointer activation, keyboard movement, and opening an item's context
menu establish the active item, and it remains active after a copy so repeated
copy actions do not require finding the row again. The active and selected
treatments are owned by `Design System §4`; their exact keys are owned by
`Input & Commands §5`.

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

### Desktop integration

bard's own right-click menu follows the conventions of a native Windows
context menu: it opens at the pointer, uses line icons, short labels,
separators and shortcut hints, stays inside the work area, and is fully
operable from the keyboard. Its exact component is `Design System §4` and its
commands are `Surfaces §1`.

Windows Explorer contributes **one** verb, **Send to Bard**. It accepts the
selected regular files, hands their canonical paths to Bard's existing
attachment pipeline, and shows the panel so the result is inspectable. It does
not recurse into folders, add per-file-type verbs, run an elevated instance, or
expose Bard's other commands. The registration and handoff mechanism are
`Technology §7`; `Input & Commands §2` owns the command path.

### Images and drawings

Images arrive by paste, by drag from another application, the composer's attach
menu, or **Send to Bard**. They are stored in bard's own folder and shown as
thumbnails. **Clicking a thumbnail or pressing the item's Copy button copies
the image back to the clipboard.**

A drawing is made in a companion window opened by the panel's persistent Draw
icon. The canvas is always white whatever the theme, because a drawing is made
to be pasted elsewhere and one drawn on a dark surface arrives unreadable.

### Export

`Export to Markdown` writes a checklist-style file to a chosen folder,
defaulting to Documents. **Phase 2:** export to an Obsidian vault, one file per
section.

---

## §4 Feature behaviour — dictation

The same-key `Ctrl Ctrl` gesture starts dictation. Holding the second Control
press is push-to-talk; a short release leaves the take locked, and a later
`Ctrl Ctrl` stops it. The text is pasted into whatever application has focus,
or lands in the timeline when the panel is open.

**Audio capture and local transcription run entirely on the machine.** The model
is selected in `Models`, which presents `Bard models`, `On this computer`, and
`Advanced model import`; the exact surface contract is `Surfaces §3.4`.

**Bard models presents the three catalogue roles defined in `Technology §4`:
Fast multilingual, Best English, and Best multilingual.** The catalogue owns
the role-to-asset mapping, conversion provenance, model-specific licence,
quantization, byte size, SHA-256, host, and relative quality/speed meters. The
role labels are product promises; the exact files and metadata are pinned in
the embedded catalogue.

**A model becomes active only after compatibility and engine-load verification;
Bard-managed downloads also require the full file's byte count and SHA-256 to
match.** Installation and activation are separate transitions: a download is
not allowed to change `model_id`, and the activation command loads the file
before it writes `model_id`. A repository that looks right but cannot run — an
unsupported architecture or a conversion in a layout the engine cannot read —
stays unavailable rather than failing during dictation. Files found in an
external Hugging Face cache can be used but are never removed by Bard. The
active model cannot be removed until another verified model is active, and
Bard never substitutes a different model when the active file becomes
unavailable. *The cost of the wrong choice:* a model that first fails during
dictation takes the most important action in the product down with it.

The model unloads after five minutes idle and reloads on demand.

A transcript is stored in the timeline only when the focused panel is its
destination. Such an item has the `dictation` origin and follows the dictation
retention policy; a transcript pasted into another application is not silently
copied into bard's history. The destination matrix is `Input & Commands §4`.

**Cloud post-processing is a separate, off-by-default capability.** A person
starts dictation explicitly, the local transcript is completed, and only then
may the enabled stage send that transcript text to the fixed OpenRouter
endpoint using the first explicitly configured model ID. The stored setting may
contain a comma-separated list, but only its first ID is sent; there is no
automatic model fallback. Audio samples, clipboard
contents, capture history, images, prompts, and unrelated local data never
enter the request. The API key is stored through keyring v1 in Windows
Credential Manager, never in SQLite, application state, events, or logs;
`Technology §8` owns the integration and secret boundary. A disabled,
unconfigured, rejected, rate-limited, timed-out, malformed, empty, or
policy-blocked cloud response leaves the local transcript untouched and uses
it as the final text. A partial cloud response is never pasted.

**Phase 2** completes the dictation settings surface: voice activity detection
and its backend, filler-word removal and its vocabulary, custom words, paste
method and delay, auto-submit, append-trailing-space, translate-to-English,
language selection, microphone muting during recording, output device, and
hardware acceleration selection. Phase 2 adds those rows to the Dictation and
Models panes.

---

## §5 Data

bard-owned application data lives in one SQLite file in the application's own
data directory, beside folders for images and Bard-managed model files. External
model caches remain where their owner placed them. Windows Credential Manager
holds the optional OpenRouter key, paid licence key, signed lease, local trial
record and installation identity; none of those values lives in SQLite.
`Technology §8` owns that boundary.

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
  origin      TEXT    NOT NULL DEFAULT 'clipboard', -- 'clipboard' | 'dictation' | 'manual'
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

-- Models downloaded through Advanced model import. Files discovered in an
-- external cache remain outside Bard's ownership and are not deleted by Bard.
CREATE TABLE custom_models (
  id            TEXT PRIMARY KEY,
  repo          TEXT    NOT NULL,
  file          TEXT    NOT NULL,
  size_mb       INTEGER NOT NULL DEFAULT 0,
  size_bytes    INTEGER NOT NULL DEFAULT 0,
  sha256        TEXT    NOT NULL DEFAULT '',
  local_path    TEXT    NOT NULL,
  ownership     TEXT    NOT NULL DEFAULT 'external',
  compatibility TEXT    NOT NULL DEFAULT 'unverified',
  created_at    INTEGER NOT NULL
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

`settings` holds `model_id`, the non-secret post-processing values
`openrouter_postprocess_enabled` (false by default) and
`openrouter_postprocess_model_ids`, the independent retention values
`clipboard_retention_days` and `dictation_retention_days` (both `30` by default),
and the non-secret audio preferences. The model-ID setting is a comma-separated
list of opaque IDs; the first ID is the only one sent to OpenRouter. The
endpoint is a fixed application constant, not a user setting. `settings` never
holds a licence key, signed lease, API key, access token, authorization header,
or secret-bearing URL.

### Retention

`origin` is the policy class, not presentation metadata. `clipboard` covers
clipboard monitoring, explicit clipboard capture and selection capture;
`dictation` covers a transcript stored because the focused panel was its
destination; `manual` covers composer notes, drawings and files deliberately
kept through an attachment or Explorer handoff.

Clipboard and dictation lifespans are independent. Each accepts **1 day, 7
days, 30 days, 90 days, 180 days, or Forever**, and each defaults to **30
days**. A favourite is exempt from automatic pruning until the person deletes
it. `manual` items are also retained until explicit deletion. Changing a
lifespan takes effect on the next successful retention pass; that pass deletes
the database rows and every Bard-owned image or attachment file that no
surviving row references. *The cost of the wrong choice:* pruning only the row
leaks storage forever, while deleting a shared or externally owned file turns a
retention preference into data loss.

The retention service runs at startup and once per day thereafter, and whenever
a lifespan changes. A failed pass leaves both rows and files untouched and
retries on the next trigger. `Input & Commands §3` owns the capture-side origin
assignment; `Surfaces §3.6` owns the settings copy.

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

**bard costs US$20 per year as a recurring annual licence.** It renews annually
until cancellation. Cancellation stops future renewals; time already paid for
remains represented by the licence service's paid-through state. A confirmed
refund is an explicit service-side entitlement change; Bard never infers or
applies refund policy locally. Bard does not sell a perpetual key or describe the
annual term as a perpetual local entitlement.

**The local trial lasts 14 consecutive days from the moment the person selects
`Start trial` during first-run onboarding.** It requires no account, licence
key or network connection and exposes every feature. A protected local trial
record fixes the start and end timestamps; it is not one of the three paid
activation slots. Removing bard's application data and protected trial
credential resets that local record, so the trial is a product experience rather
than an anti-abuse security boundary. *The cost of the wrong choice:* a
feature-limited trial of
a product whose value is cohesion demonstrates the opposite of its argument.

**One licence key activates three machines.** A desktop and two laptops are the
normal case. Bard creates a random installation identity, stores it in
Windows Credential Manager and never derives it from hardware fingerprints,
account names or other invasive machine identity. Paid activation requires an
online service round trip. The person can inspect and release activations from
the service's customer portal, or release the current installation from bard.

### The signed offline lease

A successful activation or revalidation returns an Ed25519-signed licence
lease. The application embeds the matching public verification key; the signing
private key exists only in the licence service's signing environment. The key
and lease are stored through Windows Credential Manager, never SQLite, frontend
state, events or logs.

| Lease field | Contract |
|---|---|
| `v`, `lease_version` | Integer `1`; the version pair is retained for wire compatibility |
| `product` | Constant `bard` |
| `key_id` | Non-secret identifier selecting an embedded Ed25519 public key |
| `key_fingerprint` | Lowercase SHA-256 of the licence key; it never reveals the key |
| `activation_id` | Opaque service activation identifier |
| `machine_id` | The opaque installation identifier created by this app |
| `issued_at`, `license_expires_at`, `valid_until`, `next_check_at` | Integer UTC seconds; the service sets all four |
| `entitlement` | `active`, `suspended`, or `ended` |
| `lease_sequence` | Positive integer that increases for this activation |

The wire form is
`<base64url(canonical-json)>.<base64url(ed25519-signature)>`. Canonical JSON is
UTF-8, has lexicographically sorted object keys, contains no insignificant
whitespace, and encodes timestamps as integer UTC seconds. The signature
covers the decoded canonical-JSON bytes. Bard never re-serializes a decoded
lease for verification; it verifies the original signed payload. Each build
embeds an active and next public key, selects one by `key_id`, and never fetches
a verification key at runtime. A retired key remains embedded for at least the
maximum lease lifetime so an offline lease cannot become invalid solely because
the app updated.

**A lease is valid for 30 days.** bard verifies the signature, payload version,
installation identifier, entitlement state and lease sequence. Windows UTC must
be no earlier than `issued_at` minus five minutes and no later than
`valid_until`. While the lease is valid, capture, dictation and other local creation
remain available without a network connection; update entitlement also persists,
while the download itself waits for a connection. This is a bounded offline
lease, not a promise of perpetual offline validation.

**Online revalidation is opportunistic and bounded.** bard checks after a
successful activation, at application startup when a network connection is
available, and once every 24 hours while running. It runs at most one request
at a time, gives each request 15 seconds, and makes no more than three attempts
in a rolling 24-hour period while a lease is valid. Revalidation never blocks
application startup and a timeout, transport error or service error never
invalidates an otherwise valid signed lease. A successful response
replaces the stored lease atomically; a signed response that ends or suspends
the entitlement takes effect when received. At `valid_until`, an installation
without a replacement lease becomes restricted, while existing items remain
visible, copyable and exportable and the licence surface remains reachable.
Restricted mode disables the composer, selection and clipboard capture,
dictation, drawing, file attachment and updates; it never hides, encrypts or
deletes existing local data.

The external service is Polar's [License Keys](https://polar.sh/docs/features/benefits/license-keys)
and subscription-benefit lifecycle. Bard requires a recurring annual product,
three activation instances, customer-controlled deactivation, and
subscription-state enforcement. The exact API shape is anchored to the linked
documentation; the Rust integration and every offline/failure behaviour are
owned here and specified in `Technology §8`.

---

## §7 Distribution

**miiso.dev is the channel.** The product page sells the recurring annual
licence and serves the installer directly. The consumer download is a single
signed **NSIS** installer. No MSI package is published to consumers.

**The NSIS installer is the first-run product surface.** Its custom pages use
the same name, typography, spacing and restrained language as bard: welcome,
install location, progress, and a finish page that launches the application.
The installer does not carry a separate visual system or a dense feature
solicit. First-run onboarding owns the feature, shortcut and licence-key
walkthrough in `Surfaces §2`.

**The installer and every downloaded update are signed.** An unsigned Windows
binary triggers SmartScreen, and a paid product that opens with a security
warning has lost the argument before it starts. Authenticode signing and the
updater's separate artifact signature are both Phase 1 requirements, not polish
items. The signing requirements are anchored to
[Tauri Windows signing](https://v2.tauri.app/distribute/sign/windows/) and the
[updater](https://v2.tauri.app/plugin/updater/).

**Updates are checked in-app and applied by the user.** A local trial or an
unexpired signed lease is required to download an update. The application checks
a release feed, verifies its signature, surfaces an available version, and
installs only after explicit confirmation. It never updates silently.

**The Microsoft Store is Phase 3 and is a separate distribution path.** It
reaches people who will not download an installer; its Store package does not
replace or add a second consumer installer format to the Phase 1 release.
Requirements are at
[Microsoft Store policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies).

### Crash reporting

**Phase 1 installs a local panic hook before application startup.** It writes
bounded, timestamped reports under `%LOCALAPPDATA%\com.miisodev.bard\logs`
before the release profile's `panic = "abort"` terminates the process, and the
Data pane can load the newest report. Release line tables and PDB symbols are
retained for operator symbolication; native/WebView2 failures remain outside
the Rust hook, and the artifact-retention and symbolication workflow is part of
release verification. *The cost of the wrong choice:* the one class of defect
that cannot be reproduced on the developer's machine is exactly the one that
must still leave usable evidence.

---

## §8 Legal position

**bard ships no code from either reference product.** `cooper/` is Apache-2.0
and `Handy/` is MIT; both function as behavioural references. Bard's
implementations are independent. Attribution obligations attach to code, so
none is owed — and both licences' obligations return the moment anything is
vendored, a data file included.

**No credit list appears in the product.** A paid application does not open its
settings with a list of reference products.

**Trademark position.** bard is an independent product, not affiliated with or
endorsed by shadcn, and does not use the Copper name, mark or branding. The
name, the icon and the product page are bard's own.

**Model licences are the model's, not Bard's.** Every catalogue entry records
the exact conversion repository and revision, the upstream model and revision,
the model-specific licence, the byte size, and the SHA-256 of the file. Bard's
first-party release rehosts the three default files under those terms and keeps
their provenance and notices with the assets. A model imported through
`Advanced model import` is obtained from its named source under that source's
terms; Bard claims no ownership of it. The complete role-to-asset mapping is
owned by `Technology §4`.

**The three Bard defaults have one pinned first-party host.** Their release
assets come from [miisodev/bard-models](https://github.com/miisodev/bard-models),
release `models-v2`. The files are exact-byte rehosts, not substitutions, and
carry model-specific notices alongside the upstream licence. A product release
is not accepted until those assets are published and each exact file passes the
Windows `transcribe-cpp` load-and-transcription smoke test. *The cost of the
wrong choice:* serving a paid product's defaults from another project's
bandwidth leaves Bard without control of availability, provenance, or the bytes
its checksum policy promises.

---

## §9 The product page

One page on miiso.dev. It carries what the product does, what it costs, the
system requirement, a purchase action and the signed NSIS download.

**It answers four questions in this order**, because that is the order a visitor
asks them: what is this, what does it look like, what does it cost, how do I get
it. Everything else — the local-processing promise, the model story, the
shortcut map — is supporting material below that fold.

**The commercial terms are explicit.** The price block reads: `US$20 per year.
Renews annually until cancelled.` The trial block reads: `Try every feature
free for 14 days. No account required.` The device block reads: `One licence
works on up to 3 machines.` The offline block reads: `After activation, Bard
works offline for up to 30 days between licence checks.` The page links to the
Terms and Privacy documents and makes no perpetual-licence claim.

**The local-first promise is the headline, not a footnote, and it is
conditional.** The page says: `Audio capture and transcription run on your PC.
If you enable cloud post-processing, Bard sends the completed transcript text
to OpenRouter using the model ID you enter.` It never claims that every part of
Bard remains on-device while a cloud stage is enabled.

---

## §10 Metrics

bard collects no telemetry, so every product metric comes from the sales channel
rather than the application.

| Metric | Source | What it answers |
|---|---|---|
| Annual licences sold | Payment processor | Whether the product sells |
| Renewal rate | Payment processor | Whether the first year earns a second |
| Refund rate | Payment processor | Whether it is what buyers expected |
| Page-to-purchase conversion | Site analytics on miiso.dev | Whether the page argues well |
| Trial-to-activation conversion | Licence service | Whether the local trial leads to a paid year |

**There is no in-product funnel and there will not be one.** Knowing which
features a user touches would require watching them, and the product's central
promise forbids it.

---

*Product & Business · version 1.2*
