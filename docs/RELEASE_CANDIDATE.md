# Bard release record

**Status: 0.1.1 is public on the direct channel, unsigned by decision
(2026-09-24); the Store submission is drafted.** Windows SmartScreen prompts on
the direct installer's first run until an Authenticode certificate is bought.
Microsoft signs the Store MSIX, so the Store needs nothing of Bard's own
signing.

## Channels

| Channel | State |
|---|---|
| Direct (miiso.dev) | `v0.1.1` is the latest release on the public `miisodev/bard-releases`; `/download/bard/windows` and `/api/bard/update` follow `releases/latest/download/`, so 0.1.0 installs are offered 0.1.1 |
| Microsoft Store | Reserved as **Bard: Capture & Dictation**, Store ID `9N37XJJS1F3H`; identity `miisodev.BardCaptureDictation` / `CN=61F791B1-D87E-4C90-9617-536200345DA6` / `miisodev`. Submission 1 is a draft; `Bard_0.1.1_x64_Store.msix` and the listing copy are ready (`docs/STORE_SUBMISSION.md`). Submit by 2026-12-24 or the reservation lapses |
| Commerce | Polar one-time product `d019d40d-218d-4e76-8323-add0822b0c3f`, $20 USD; licence benefit with 3 activations and no Polar-side expiry — the broker stamps 365 days at first activation; the downloadable benefit carries the 0.1.1 installer. The subscription product it replaced is archived |

## What is waiting

| Gate | Needs | Owner |
|---|---|---|
| Store submission | Fill the drafted sections from `docs/STORE_SUBMISSION.md`, answer the age-rating questionnaire, submit for certification | Miiso — the attestations are the account holder's |
| First purchase | One real purchase, activation on the direct and Store copies | Miiso |
| Authenticode | DigiCert (account open) or another individual-validation certificate | Miiso, later |
| Expiry reminders | A daily job emailing keys nearing `expiresAt` — before day 335 | Here |
| Installed matrix | Clean-machine install, upgrade, uninstall, 150% scaling, screen reader | Here |

## Published builds

| Version | Commit | Installer SHA-256 | Notes |
|---|---|---|---|
| 0.1.1 | `f03d7ba` | `2B89351F4509A5150D9600AE112C1C60168B125725D0B58071C5AE8174AB08B7` | Draw ink follows the page |
| 0.1.0 | `7c21e54` | `2074150ED57028DB3EB5650DCF58E6166512EA8F6998B4EAAF7E3CBD0DE86353` | First public build |

Store package 0.1.1: `9132E61E91B746F002C58D642D8A3C68905B5C1936AFF9DEC9CD7D733FFBFCE3`.
Built with `scripts/build-release.ps1 -Unsigned`; Rust tests **63 passed** ·
strict Clippy **pass** · TypeScript **pass** · Authenticode **NotSigned**, by
decision.

The earlier `rc-0.1.0` and `rc-0.1.0-r2` tags mark unsigned local builds from
before the review pass. Both are superseded; neither was published.
