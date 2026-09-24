# Bard 0.1.0 release candidate record

**Status: NO-GO — intentionally unsigned**

- Source commit: `4249b10588283ac521af7e1bbf858fc9fbdb8883`
- Local tag: `rc-0.1.0` (not pushed)
- Product branch: `main` (not pushed; the signed-release decision is pending)
- Build date: 2026-09-24
- Installer: `src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe`
- Installer SHA-256: `5CF93BC04A671B84A8DD9C58D9369961B118BFC6F19235ECC2FAC3F63BFD6453`
- Executable SHA-256: `7116DD19C998BFEB94B32F04FDA94EDDCF4B442A642B93DE3C75125F06E9FBA9`
- Updater `.sig` SHA-256: `C5D7A7E0828B9DC4D04E5DABF109A0955BDD6D8EC73F02115091B727049901C3`
- NSIS integrity: **pass** (`7z t`)
- Tauri/minisign updater signature: **pass** (verified against `src-tauri/tauri.conf.json`)
- Rust tests: **59 passed**
- Strict Clippy: **pass**
- Frontend build/CSS audit: **pass**
- Bun dependency audit: **pass**
- Authenticode executable: **NotSigned**
- Authenticode installer: **NotSigned**

The user elected to keep this candidate unsigned. Do not upload the binary
or `.sig` to the public release repository, promote paid checkout, or change
`miiso.dev` production URLs to point at this candidate. The release repository
contains policy documentation only.

## Reopen condition

Resume the release gate only after a trusted Authenticode identity is
available. Follow `docs/RELEASE_RUNBOOK.md`: sign both the executable and NSIS
installer, rerun `scripts/verify-release.ps1 -RequireSignature`, publish the
signed assets and updater manifest, configure the production download URLs,
and complete the installed/live commerce smoke test.
