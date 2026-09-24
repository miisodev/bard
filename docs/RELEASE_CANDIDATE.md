# Bard 0.1.0 release record

**Status: the direct download is public and unsigned by decision (2026-09-24);
the Store product is reserved.** Windows SmartScreen prompts on the direct
installer's first run until an Authenticode certificate is bought. Microsoft
signs the Store MSIX, so the Store needs nothing of Bard's own signing.

## Channels

| Channel | State |
|---|---|
| Direct (miiso.dev) | `Bard_0.1.0_x64-setup.exe` on the public `miisodev/bard-releases` release `v0.1.0`; `/download/bard/windows` redirects to it |
| Microsoft Store | Reserved as **Bard: Capture & Dictation**, Store ID `9N37XJJS1F3H`; identity `miisodev.BardCaptureDictation` / `CN=61F791B1-D87E-4C90-9617-536200345DA6` / `miisodev`; must be submitted within three months of 2026-09-24 |
| Commerce | Polar one-time product `d019d40d-218d-4e76-8323-add0822b0c3f`, $20 USD; licence benefit with 3 activations and no Polar-side expiry — the broker stamps 365 days at first activation. The subscription product it replaces is archived |

## What is waiting

| Gate | Needs | Owner |
|---|---|---|
| Store submission | Listing text, clean screenshots, age rating, upload of the Store MSIX, submit | Here, with Miiso for anything Partner Center asks of the account holder |
| First purchase | One real purchase, activation on the direct and Store copies | Miiso |
| Authenticode | DigiCert (account open) or another individual-validation certificate | Miiso, later |
| Expiry reminders | A daily job emailing keys nearing `expiresAt` — before day 335 | Here |
| Installed matrix | Clean-machine install, upgrade, uninstall, 150% scaling, screen reader | Here |

## Published build

- Commit `7c21e54`, built with `scripts/build-release.ps1 -Unsigned`
- Hashes: see `SHA256SUMS` on the `v0.1.0` release
- Rust tests: **63 passed** · strict Clippy: **pass** · TypeScript: **pass**
- Authenticode: **NotSigned**, by decision

The earlier `rc-0.1.0` and `rc-0.1.0-r2` tags mark unsigned local builds from
before the review pass. Both are superseded; neither was published.
