# Bard 0.1.0 release candidate record

**Status: NO-GO — intentionally unsigned**

- Source commit: `4249b10588283ac521af7e1bbf858fc9fbdb8883`
- Local tag: `rc-0.1.0` (not pushed)
- Product branch: `main` (not pushed; the signed-release decision is pending)
- Build date: 2026-09-24
- Installer: `src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe`
- Installer SHA-256: `5CF93BC04A671B84A8DD9C58D9369961B118BFC6F19235ECC2FAC3F63BFD6453`
- MSIX: `src-tauri/target/release/bundle/msix/Bard_0.1.0_x64.msix`
- MSIX SHA-256: **not a release hash**; unsigned local pack metadata changes on rebuild
- Executable SHA-256: `7116DD19C998BFEB94B32F04FDA94EDDCF4B442A642B93DE3C75125F06E9FBA9`
- Updater `.sig` SHA-256: `C5D7A7E0828B9DC4D04E5DABF109A0955BDD6D8EC73F02115091B727049901C3`
- NSIS integrity: **pass** (`7z t`)
- Tauri/minisign updater signature: **pass** (verified against `src-tauri/tauri.conf.json`)
- Rust tests: **59 passed**
- Strict Clippy: **pass**
- Frontend build/CSS audit: **pass**
- Bun dependency audit: **pass**
- Authenticode executable: **NotSigned**
- Authenticode NSIS installer: **NotSigned**
- MSIX package: **structurally valid, unsigned**

The release remains intentionally unsigned while the public direct-download
certificate is being obtained. Do not upload the binary, MSIX, or `.sig` to the
public release repository, promote paid checkout, or change `miiso.dev`
production URLs to point at this candidate. The release repository contains
policy documentation only.

## Reopen condition

Resume the release gate only after a trusted publicly trusted code-signing PFX
is available. Follow `docs/RELEASE_RUNBOOK.md`: sign the executable, NSIS
installer, and MSIX package, rerun `scripts/verify-release.ps1
-RequireSignature -RequireMsix`, publish the signed assets and updater manifest,
submit the MSIX package to Microsoft Store, configure both production download
URLs, and complete the installed/live commerce smoke test.
