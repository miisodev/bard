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
  -PublisherDisplayName "miisodev" `
  -DisplayName "Bard: Capture & Dictation"
```

The product is reserved as **Bard: Capture & Dictation** (Store ID
`9N37XJJS1F3H`, `https://apps.microsoft.com/detail/9N37XJJS1F3H`) — plain
"Bard" was taken. The values come from **Partner Center › Product identity**;
the package display name must be one of the reserved names, while the tile and
Start menu keep the app's own name, Bard. The Store rejects a package whose identity differs, so they
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
| Licence | Same 14-day trial and one-time $20 licence (365 days from activation), bought on miiso.dev | Same |

The application reads its package identity at runtime and reports the Store
channel in About.

## Partner Center checklist

- Product name reserved 2026-09-24; submit within three months or the
  reservation lapses.
- Privacy policy URL: `https://miiso.dev/privacy`.
- Pricing: list Bard as free to install; the licence is purchased on miiso.dev.
  Declare the in-app purchase path in the submission notes.
- At least one real Bard screenshot and the required Store logo assets; never
  a development-only capture.
- System requirements: Windows 10 2004 (build 19041) or later, x64, WebView2
  runtime.
- Upload the MSIX package, never the NSIS installer.
- Complete privacy, age rating, pricing/availability and submission notes
  before requesting certification.
- Test the Store-installed copy on a clean Windows account before release.

## Submission 1 — what goes in each section

Submission 1 is open as a draft in Partner Center. The package and four
screenshots (1600 × 900, real Bard windows on a demo profile) are in
`Downloads\Bard Store submission`.

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

**Packages.** `Bard_0.1.1_x64_Store.msix`, Desktop only.

**Store listing (English).**

- Short description: *Capture, clipboard and offline dictation in one
  keyboard-first panel.*
- Description:

  > Bard keeps the fragments worth keeping and the words you would rather say
  > than type, in one keyboard-first panel.
  >
  > Capture: select text anywhere and tap Shift twice, and it lands in Bard's
  > timeline. Paste images, drop files, sort notes into sections, mark them
  > done and copy any of them back with a keystroke — or paste your first three
  > straight into the app you are in with Right Shift twice and 1, 2 or 3.
  >
  > Dictate: tap Ctrl twice and speak. Bard transcribes on your PC with a local
  > speech model, so your audio never leaves the machine, and pastes the text
  > where you were typing. Hold the second Ctrl for push-to-talk.
  >
  > Draw: sketch a quick diagram beside your notes, then copy or save it as an
  > image.
  >
  > Refine (optional): send only the finished transcript to a model you choose
  > through OpenRouter to tidy it up. Off by default.
  >
  > Every feature is free for 14 days. To keep using Bard, buy a licence at
  > miiso.dev: $20 once, for 365 days from the day you activate it, on up to
  > three PCs. It never renews on its own.

- Features: Capture any selection with Shift + Shift · Offline dictation with a
  local speech model · Paste your first three items anywhere · Sections, done
  state, favourites and sort · Quick diagrams, copied as images · Light, dark
  and custom backgrounds
- Keywords: dictation, speech to text, clipboard, capture, notes, offline,
  whisper
- Copyright: © 2026 miisodev

**Submission options — notes for certification.**

> Bard is a full-trust desktop app (runFullTrust is needed for its global
> shortcuts, clipboard capture and pasting into the active app). Every feature
> works for 14 days with no account or key. After that, a licence bought on
> https://miiso.dev activates it; as a non-game app it uses its own commerce
> (policy 10.8.2). To try it: select text in any app and tap Shift twice to
> capture it; tap Ctrl twice to dictate (onboarding downloads the local voice
> model).

## Release relationship

The Store listing and `/tools/bard` describe the same version, price, trial,
local-first boundary, privacy disclosures and support contact. The release
record holds the source commit and tag, the installer and MSIX SHA-256 hashes,
the package identity and version, the Partner Center certification receipt, and
clean-machine install, launch and uninstall evidence.

The Store channel is live only when the listing is published and
`NEXT_PUBLIC_BARD_STORE_URL` points at it.
