# Bard release record

**Status: 0.1.2 is public on the direct channel, unsigned by decision
(2026-09-24); the Store submission is in certification.** Windows SmartScreen
prompts on the direct installer's first run, and the product page tells buyers
how to dismiss it; no Authenticode certificate is being bought. Microsoft signs
the Store MSIX, so the Store needs nothing of Bard's own signing.

## Channels

| Channel | State |
|---|---|
| Direct (miiso.dev) | `v0.1.2` is the latest release on the public `miisodev/bard-releases`; `/download/bard/windows` and `/api/bard/update` follow `releases/latest/download/`, so earlier installs are offered 0.1.2 |
| Microsoft Store | Reserved as **Bard: Capture & Dictation**, Store ID `9N37XJJS1F3H`; identity `miisodev.BardCaptureDictation` / `CN=61F791B1-D87E-4C90-9617-536200345DA6` / `miisodev`. Submission 1 is submitted and in certification; the public page still returns HTTP 410 until certification completes (`docs/STORE_SUBMISSION.md`). `NEXT_PUBLIC_BARD_STORE_URL` waits on the published listing. Submit by 2026-12-24 or the reservation lapses |
| Commerce | Polar one-time product `d019d40d-218d-4e76-8323-add0822b0c3f`, $20 USD; licence benefit with 3 activations and no Polar-side expiry — the broker stamps 365 days at first activation; the downloadable benefit carries the 0.1.2 installer. The subscription product it replaced is archived |

## What is waiting

| Gate | Needs | Owner |
|---|---|---|
| Store certification | Track submission 1 through certification, answer any rejection, publish the listing and set `NEXT_PUBLIC_BARD_STORE_URL` | Miiso — the attestations are the account holder's |
| First purchase | One real purchase, activation on the direct and Store copies | Miiso |
| Authenticode | None by decision — no certificate is bought; revisit only if the SmartScreen notice costs buyers | Miiso, only if it ever comes back |
| Expiry reminders | A daily job emailing keys nearing `expiresAt` — before day 335 | Here |
| Installed matrix | Clean-machine install, upgrade, uninstall, 150% scaling, screen reader | Here |

## Published builds

| Version | Commit | Installer SHA-256 | Notes |
|---|---|---|---|
| 0.1.2 | `a2e66b0` | `21A74A17E97652699A5BFDA891230ECDE5D88317C30660F4306A7F88342E78EB` | Report route for Refine output (Store generative-AI requirement) |
| 0.1.1 | `f03d7ba` | `2B89351F4509A5150D9600AE112C1C60168B125725D0B58071C5AE8174AB08B7` | Draw ink follows the page |
| 0.1.0 | `7c21e54` | `2074150ED57028DB3EB5650DCF58E6166512EA8F6998B4EAAF7E3CBD0DE86353` | First public build |

Store package 0.1.2: `979ABD271CD0BB5CC6D4388799A2F95CFA7E7C4B5A9DADD5C2929D1EAECC12F1`.
Built with `scripts/build-release.ps1 -Unsigned`; Rust tests **63 passed** ·
strict Clippy **pass** · TypeScript **pass** · Authenticode **NotSigned**, by
decision.

The earlier `rc-0.1.0` and `rc-0.1.0-r2` tags mark unsigned local builds from
before the review pass. Both are superseded; neither was published.
