# Bard Microsoft Store submission

The Store channel is an **MSIX** package built from the same signed release as
the direct NSIS installer. It is the only MSIX Bard ships; nothing is offered
for sideloading.

## Package

Build the signed release first (`docs/RELEASE_RUNBOOK.md` §3), then:

```powershell
.\scripts\build-msix.ps1 -Store `
  -PackageName "<Package/Identity/Name>" `
  -Publisher "<Package/Identity/Publisher>" `
  -PublisherDisplayName "<publisher display name>"
```

All three values come from **Partner Center › Product identity** after the
name is reserved. The Store rejects a package whose identity differs, so they
are never guessed. The package is a Windows desktop full-trust MSIX containing
the signed `bard.exe` and the Store assets, with a `windows.startupTask`
extension (`BardStartup`) for login start. It is uploaded unsigned: Microsoft
signs it after certification. The Windows SDK validates the manifest while
packing; Partner Center and the Windows App Certification Kit validate it again.

## What the Store copy does differently

| Behaviour | Store copy | Direct copy |
|---|---|---|
| Updates | Delivered by the Store; the in-app updater stands down | Tauri updater, user-confirmed |
| Login start | Startup task, switched in Settings › Apps › Startup | Run key, switched in Settings › General |
| Explorer `Send to Bard` | Not registered | Registered by the installer |
| Licence | Same 14-day trial and $20 annual licence, bought on miiso.dev | Same |

The application reads its package identity at runtime and reports the Store
channel in About.

## Partner Center checklist

- Reserve the product name; copy the identity values into the build command.
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
