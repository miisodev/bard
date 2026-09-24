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

## Release relationship

The Store listing and `/tools/bard` describe the same version, price, trial,
local-first boundary, privacy disclosures and support contact. The release
record holds the source commit and tag, the installer and MSIX SHA-256 hashes,
the package identity and version, the Partner Center certification receipt, and
clean-machine install, launch and uninstall evidence.

The Store channel is live only when the listing is published and
`NEXT_PUBLIC_BARD_STORE_URL` points at it.
