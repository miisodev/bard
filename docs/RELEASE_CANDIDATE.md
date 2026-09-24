# Bard 0.1.0 release record

**Status: the Store channel can proceed now; the direct download waits for the
certificate.** Microsoft signs the Store MSIX, so the Store needs nothing of
Bard's own signing. The direct installer ships only as the first signed build
from `scripts/build-release.ps1`; nothing unsigned is published.

## What is waiting

| Gate | Needs | Owner |
|---|---|---|
| Microsoft Store — now | Reserve the name; send the Product identity values (Identity Name, Publisher — expected `CN=61F791B1-D87E-4C90-9617-536200345DA6` — and display name `miisodev`); listing assets; submit | Miiso reserves and submits; package and assets built here |
| Commerce — now | A one-time $20 Polar product with a licence-key benefit (3 activations, **no expiry** — the broker starts 365 days at first activation); production checkout on; one real purchase, activation and refund | Miiso in Polar; the broker and the run are here |
| Site — now | Push `6791e95` and `f2a4a29` (Store link, one-time licence copy) and set `NEXT_PUBLIC_BARD_STORE_URL` once the listing is live | Miiso approves the push |
| Authenticode — later | DigiCert (account open) or another individual-validation certificate on a token or cloud signer | Miiso |
| Expiry reminders — before day 335 | A daily job emailing keys nearing `expiresAt` | Here |
| Installed matrix | Clean-machine install, upgrade, uninstall, 150% scaling, screen reader, physical gesture check | Here, with one physical pass by Miiso |

## Last local build (test only)

- Built 2026-09-24 from the commit that adds Draw's page, standalone windows and the fixed gestures
- Installer: `src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe`
- Installer SHA-256: `5DBC559E60F7969D54E732A2DE6E1F5BA378B4E2AE02A3599266F5622DE9813C`
- Executable SHA-256: `4FC05F1A4DD1881CAADC938DE64A0F290B8F2E0486124D8FA7225CDB5FDDDF93`
- Rust tests: **60 passed** · strict Clippy: **pass** · frontend build and CSS audit: **pass**
- Installed and exercised on this machine: `Send to Bard` registered, roaming data
  migrated (5 images, active model), companions measured flush, light and dark,
  Draw's canvas sheet, custom wells, F11 fullscreen and Esc
- Authenticode: **NotSigned** — never upload, publish or link this build

The earlier `rc-0.1.0` and `rc-0.1.0-r2` tags mark unsigned local builds from
before the review pass. Both are superseded; neither was published.
