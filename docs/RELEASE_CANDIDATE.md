# Bard 0.1.0 release candidate record

**Status: NO-GO — intentionally unsigned**

- Source commit: `9150e80b344230eb244ec30d23e0a33283ef1fa1`
- Local tag: `rc-0.1.0-r2` (not pushed)
- Product branch: `main` (not pushed; the signed-release decision is pending)
- Build date: 2026-09-24
- Installer: `src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe`
- Installer SHA-256: `EF937D0215AA3527AC29B79AFE4D705F0A93840A290A4EF36734A259C72AFBF1`
- MSIX: `src-tauri/target/release/bundle/msix/Bard_0.1.0_x64.msix`
- MSIX SHA-256: **not a release hash**; unsigned local pack metadata changes on rebuild
- Executable SHA-256: `0B18F3BE5EC3E3AE1035882C804A2E667FAB4EE1883168F217517563E71FD5D3`
- Updater `.sig` SHA-256: `7D1FD8C957B59F9C0F4F3E166AC2F54EF837D8216DF5A3B4F3E3252954C48EBB`
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
