# Bard 0.1.0 release record

**Status: NO-GO — no release candidate yet.** The first candidate is the first
signed build from `scripts/build-release.ps1`; nothing unsigned is a candidate.

## What is waiting

| Gate | Needs | Owner |
|---|---|---|
| Authenticode | A publicly trusted code-signing certificate on a USB token or cloud signer (a PFX can no longer be issued) | Miiso |
| Microsoft Store | Name reservation and the Partner Center identity values for `build-msix.ps1 -Store`, listing assets, submission | Miiso, with the package built here |
| Commerce | A real Polar purchase, renewal, refund and three-activation run | Miiso pays; the run is scripted here |
| Installed matrix | Clean-machine install, upgrade, uninstall, 150% scaling, screen reader | Here |

## Last local build (test only)

- Built 2026-09-24 from the working tree committed after the review pass
- Installer: `src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe`
- Installer SHA-256: `AC695D841F27222467E905644FA050AA2688C662F339E813077BC945000BA8B6`
- Executable SHA-256: `73FEBAE1D0E465E2B5B624C22895FA3856C38F5AE99DEA865DB3BCFB4C6C78FA`
- Rust tests: **64 passed** · strict Clippy: **pass** · frontend build and CSS audit: **pass**
- Installed and exercised on this machine: `Send to Bard` registered, roaming data
  migrated (5 images, active model), companions measured flush, light and dark
- Authenticode: **NotSigned** — never upload, publish or link this build

The earlier `rc-0.1.0` and `rc-0.1.0-r2` tags mark unsigned local builds from
before the review pass. Both are superseded; neither was published.
