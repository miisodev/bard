# Bard Microsoft Store submission

The Store channel is an **MSIX** package built from the same release as the
direct NSIS installer. It is the only MSIX Bard ships; nothing is offered
for sideloading.

## Package

Build the release first (`docs/RELEASE_RUNBOOK.md` §3), then:

```powershell
.\scripts\build-msix.ps1 -Store `
  -PackageName "miisodev.BardCaptureDictation" `
  -Publisher "CN=61F791B1-D87E-4C90-9617-536200345DA6" `
  -PublisherDisplayName "miiso.dev" `
  -DisplayName "Bard: Capture & Dictation"
```

The product is reserved as **Bard: Capture & Dictation** (Store ID
`9N37XJJS1F3H`, `https://apps.microsoft.com/detail/9N37XJJS1F3H`) — plain
"Bard" was taken. The values come from **Partner Center › Product identity**;
the package display name must be one of the reserved names, while the tile and
the Start menu keep the app's own name, Bard. The Store rejects a package whose identity differs, so they
are never guessed. The package is a Windows desktop full-trust MSIX containing
`bard.exe` and the Store assets, with a `windows.startupTask`
extension (`BardStartup`) for login start. It is uploaded unsigned: Microsoft
signs it after certification. The Windows SDK validates the manifest while
packing; Partner Center and the Windows App Certification Kit validate it again.

## What the Store copy does differently

| Behaviour | Store copy | Direct copy |
|---|---|---|
| Updates | Delivered by the Store; the in-app updater stands down | Tauri updater, user-confirmed |
| Login start | Startup task, switched in Settings › Apps › Startup | Run key, switched in Settings › General |
| Explorer `Send to Bard` | Not registered | Registered by the installer |
| Licence | Same 14-day trial and one-time $20 licence (365 days from first activation), bought on miiso.dev | Same |

The direct copy's installer is **unsigned by decision** (2026-09-24) and the
product page tells buyers to expect the SmartScreen prompt. Nothing in this
listing may imply the direct installer is signed, and the Store copy does not
meet SmartScreen at all because Microsoft signs the package.

The application reads its package identity at runtime and reports the Store
channel in About.

## Partner Center checklist

- Product name reserved 2026-09-24; submit within three months or the
  reservation lapses.
- Privacy policy URL: `https://miiso.dev/privacy`.
- Pricing: list Bard as free to install; the licence is purchased on miiso.dev.
  Declare the in-app purchase path in the submission notes.
- At least one real Bard screenshot and the required Store logo assets; never
  a development-only capture. See **Screenshots** and **Icon and tile** below
  for what is still missing.
- System requirements: Windows 10 2004 (build 19041) or later, x64, WebView2
  runtime.
- Upload the MSIX package, never the NSIS installer.
- Complete privacy, age rating, pricing/availability and submission notes
  before requesting certification.
- Test the Store-installed copy on a clean Windows account before release.

## Submission 1 — what went in

Submission 1 of Store ID `9N37XJJS1F3H` is submitted and in certification. The
package and four screenshots (1600 × 900, real Bard windows on a demo profile)
were in `Downloads\Bard Store submission`. The listing returns HTTP 410 until
Microsoft publishes it, and the same sections are re-read if certification asks
for a change.

**Pricing and availability.** All markets · public · discoverable · release as
soon as possible · base price **Free** · no free trial (Bard's own 14-day trial
covers it).

**Properties.** Category **Productivity**. Privacy policy
`https://miiso.dev/privacy` · website `https://miiso.dev/tools/bard` · support
`miisodev@gmail.com`. Declarations: **This product allows users to make purchases, but does not
use the Microsoft Store commerce system** and **This product incorporates
generative AI features** (Refine) — checked; Settings › Refine › Report and
About › Support open miiso.dev/tools/bard#support, the reporting route the
Store requires of generative AI features. Unchecked: accessibility-tested (not
yet tested), OneDrive backups (Bard's data stays on the machine), recording and
broadcast (games only), pen and ink (untested).

**Age ratings.** No to every content question. Interactive elements: digital
purchases **yes** (the licence is bought on miiso.dev); users interact, shares
location, unrestricted internet — **no**.

**Packages.** `Bard_0.1.2_x64_Store.msix`, Desktop only.

**System requirements.** Memory 4 GB minimum, 8 GB recommended (the local
speech model runs in memory). Processor: 64-bit, 2 cores minimum; 64-bit,
4 cores recommended. DirectX, video memory and graphics: not specified.

**Store listing (English).**

- Short description: *Capture. Dictate. Keep.*
- Description:

  > Bard is one keyboard-first panel for the two things you already do
  > separately: keeping what you copy, and saying what you would rather not
  > type. Both land in the same searchable timeline, in the same sections, and
  > both paste back with the same keystroke. That is the whole product — not
  > two utilities running side by side.
  >
  > **Capture and dictate, together.** Select text or an image in any app and
  > tap Left Shift twice; it is in Bard. Tap Ctrl twice and speak; Bard
  > transcribes on this PC with a local speech model, so your audio never
  > leaves the machine, and the text is pasted where you were typing or kept in
  > the same timeline as your captures. Hold the second Ctrl for push-to-talk,
  > or tap it again to stop. One shortcut map covers both halves, so nothing
  > collides and nothing has to be learned twice.
  >
  > **The clipboard half, in full.** Everything you copy anywhere lands in a
  > timeline you can search. Paste your first three items straight into the app
  > you are in with Right Shift twice and then 1, 2 or 3. Sort into sections,
  > mark things done, keep favourites, drag files and images in, edit a line
  > inline, export to Markdown, and let older items expire on their own
  > schedule.
  >
  > **Draw, when a sketch says it faster.** A quick diagram beside your notes —
  > pen, line, arrow, rectangle, ellipse, text — copied or saved as an image
  > straight into the timeline.
  >
  > **Refine, optional and off by default.** Turn it on and Bard sends only
  > the finished transcript, never the audio, to one model you choose through
  > OpenRouter, to tidy the wording. Off, or if the request fails, the local
  > transcript is used unchanged.
  >
  > Everything works free for 14 days, with no account and no key. To keep
  > going, buy a licence once at miiso.dev: $20, good for 365 days from the day
  > you activate it, on up to three PCs. It never renews and nothing is charged
  > again.

- Features: Capture any selection with Left Shift + Left Shift · Offline
  dictation with a local speech model · One timeline and one shortcut map for
  both · Paste your first three items anywhere · Sections, done state, favourites
  and sort · Quick diagrams, copied as images · Light, dark and custom
  backgrounds
- Keywords: dictation, speech to text, clipboard, capture, notes, offline,
  whisper
- Copyright: © 2026 miisodev

## Listing decisions

**The one-liner is `Capture. Dictate. Keep.`** and it is the canonical sentence
for Bard everywhere. It already ships in `src-tauri/tauri.conf.json`
(`shortDescription`), it is what Settings › About shows, and it is the README's
first line. Chosen over the alternatives because it is on the three surfaces a
person meets first, it is 22 characters so the Store's short-description field
carries it whole, and it names the two halves without promising a fourth thing.

Two surfaces still carry a different sentence and should converge on it:

| Surface | Says | Should say |
|---|---|---|
| `src-tauri/tauri.conf.json` `longDescription` | "Bard is a minimal native Windows workspace that merges clipboard capture, offline voice dictation and optional text cleanup into one keyboard-first panel." | the same clause, naming **Refine** instead of "text cleanup" |
| `src/components/Onboarding.tsx` step 1 | "Capture anything you copy. Dictate offline with your own voice. Captures stay on this machine, and cloud text cleanup is always opt-in." | keep the local-first sentence; name **Refine** rather than "cloud text cleanup" |

**Every gesture in Store copy says which Shift.** Capture is **Left** Shift
twice; Right Shift twice toggles the panel, and Right Shift twice then 1/2/3
pastes an item. A customer who taps the wrong Shift concludes the product is
broken, and the listing is read before the product is ever installed.
`src-tauri/src/gesture.rs` is the authority: `LeftShift → Capture`,
`RightShift → TogglePanel`, `LeftAlt → ToggleDraw`, `RightAlt → ToggleSettings`,
and Control drives dictation from either key.

**The description leads with cohesion, not with four features.** The earlier
draft gave Capture, Dictate, Draw and Refine four equal paragraphs, which
describes accumulation. The product's claim is that dictated text and captured
text are the same kind of thing and live in one surface under one shortcut map;
the opening now says that, Draw and Refine are demoted to the optional extras
they are, and the price is still last.

## Screenshots — required, and there is no manifest

Submission 1 went up with four 1600 × 900 captures of real Bard windows on a
demo profile, and Partner Center accepted them into certification. That is the
only dimension claim here with evidence behind it. **Confirm every other size
against the Partner Center listing form before submission 2 rather than
assuming**, and write the answer down here when it is known.

Nothing in this repository records which capture belongs to which surface, at
what size, in which theme, from which build, or whether it shows real data.
`scripts/shot.ps1` can produce them; it does not decide which belong in a
listing. This checklist is the missing artefact.

- [ ] **A manifest**, one row per submitted image: file, surface (panel,
      Settings, Draw, dictation overlay, onboarding, About/licence), pixel
      size, theme, display scale, capture date, and the build's commit.
- [ ] **Size matrix decided and written down.** 1600 × 900 is proven. Record
      whether the listing also wants a second size or aspect, and which one the
      Store's tile crops to.
- [ ] **Composition.** The panel is 380 × 620 and the Settings companion
      720 × 620 — both portrait. A 16:9 landscape capture of a portrait window
      is mostly empty desktop. Decide whether captures are cropped to the
      window, composed with real desktop context, or placed in a device frame,
      and apply the same answer to all of them.
- [ ] **Coverage.** At minimum: the panel with items, the empty state, the
      settings surface, and dictation in progress. Decide whether Draw and the
      onboarding are listed at all.
- [ ] **Themes and scaling.** Decide whether the set is all light, all dark, or
      split, and whether any capture is taken at 150% scaling.
- [ ] **No development captures.** No `tauri dev` window, no debug build, no
      placeholder rows, no real customer data, and nothing staged that the
      shipped build cannot produce.
- [ ] **Captions.** Every caption uses the same gesture wording as the listing:
      **Left** Shift to capture, Right Shift to toggle the panel.
- [ ] **Hidden-window capture caveat.** `shot.ps1` cannot capture a hidden
      window without `-Show`, and `-Show` raises it with `ShowWindow`, which
      Tauri does not know about — the window is then visible to Windows and
      hidden to the app, so it cannot subsequently be hidden by the app and
      auto-hide looks broken. Fine for a listing still, wrong for any
      behavioural claim. Summon the panel the way the app does (a second
      instance) before capturing anything that is about showing and hiding.

## Icon and tile — required, and there is no plan

`scripts/gen-icon.mjs` writes one flat 1024 × 1024 tile to `app-icon.png`.
`src-tauri/icons/` holds flat rasters (`32x32` through
`Square310x310Logo`, plus `.ico` and `.icns`). `scripts/build-msix.ps1` copies
exactly four of them — `StoreLogo`, `Square44x44Logo`, `Square150x150Logo`,
`Square310x310Logo` — and the manifest declares `uap:VisualElements` with a
44 px and a 150 px logo and a transparent background.

That is a working legacy desktop tile and it is what submission 1 shipped. It
is not a deliberate icon plan, and the gap is worth naming before submission 2:

- [ ] **Decide flat tile vs layered/adaptive icon.** A flat 150 px raster scaled
      up by the Start menu reads soft on a high-DPI display. A layered icon
      supplies a background layer, a foreground layer and a monochrome layer so
      Windows can compose and re-tint it. Write down which Bard ships and why,
      instead of letting the file list decide.
- [ ] **If layered**, add the layer assets and the manifest entries they need
      (a target-size/branding block, and a `Wide310x150Logo` if the wide tile is
      wanted), and extend `build-msix.ps1`'s hard-coded four-name asset array.
      Whichever is chosen, the generator and the manifest must not be able to
      disagree: `gen-icon.mjs` writes `app-icon.png` and nothing in this
      repository regenerates `src-tauri/icons/`, so today there are two
      independent sources of the same brand mark.
- [ ] **Tile background.** The manifest declares `BackgroundColor="transparent"`
      but supplies no background layer, so the tile is whatever the flat raster
      contains. Decide whether transparency is intended for the Store tile.
- [ ] **Store logo.** `StoreLogo.png` exists and is what the manifest's `<Logo>`
      points at. Confirm the listing form's own logo asset is uploaded
      separately and that the two match.
- [ ] **Monochrome.** Confirm whether the listing asks for a monochrome variant
      and whether Windows' tinted-icon setting should be supported.
- [ ] **Naming.** The reserved listing name is "Bard: Capture & Dictation"; the
      tile, the Start menu and the manifest's `Application Id` keep the app's
      own name, Bard. That split is deliberate — record it so a future rename
      does not "fix" the tile and break certification.

**Submission options — notes for certification.**

> Bard is a full-trust desktop app (runFullTrust is needed for its global
> shortcuts, clipboard capture and pasting into the active app). Every feature
> works for 14 days with no account or key. After that, a licence bought on
> https://miiso.dev activates it; as a non-game app it uses its own commerce
> (policy 10.8.2). To try it: select text in any app and tap **Left** Shift
> twice to capture it; tap Ctrl twice to dictate (onboarding downloads the local
> voice model). The licence is a single $20 payment for 365 days from first
> activation on up to three PCs; it is not a subscription and nothing renews
> or charges again.

## Release relationship

The Store listing and `/tools/bard` describe the same version, price, trial,
local-first boundary, privacy disclosures and support contact. The release
record holds the source commit and tag, the installer and MSIX SHA-256 hashes,
the package identity and version, the Partner Center certification receipt, and
clean-machine install, launch and uninstall evidence.

The Store channel is live only when the listing is published and
`NEXT_PUBLIC_BARD_STORE_URL` points at it.
