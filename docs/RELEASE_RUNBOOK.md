# Bard release runbook

This runbook is for an operator releasing the native Windows application. It
assumes the source commit is frozen and that the release is **not** eligible
until the Authenticode gate passes.

## 1. Freeze

```powershell
git status --short
git rev-parse HEAD
git tag -a v0.1.0 <commit> -m "Bard 0.1.0"
```

Do not build a release from a dirty worktree. The commit, source archive,
`Cargo.lock`, and frontend lockfile must describe the same tree.

## 2. Gate the source

```powershell
bun install --frozen-lockfile
bun run build
bun audit
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

A green build is not a release approval. Keep the output with the release
record.

## 3. Build NSIS and updater signature

The updater private key is supplied through `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; it must never be committed.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$HOME\.tauri\bard-updater.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content "$HOME\.tauri\bard-updater-password.txt" -Raw).Trim()
$env:BARD_NO_GUARDIAN = "1"
bun run tauri build --bundles nsis
```

The only consumer artifact is:

```text
src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe
```

Reject any MSI or additional installer format from the release stage.

## 4. Authenticode

Obtain the publisher certificate through the approved commercial signing
provider. Keep the PFX and password outside the repository and CI logs.

```powershell
.\scripts\sign-release.ps1 `
  -Version 0.1.0 `
  -CertificatePath "$HOME\.secrets\bard-code-signing.pfx"
.\scripts\verify-release.ps1 -Version 0.1.0 -RequireSignature
```

The verifier checks the NSIS archive, Authenticode status for both binaries,
the Tauri/minisign updater signature, the embedded updater public key, and the
artifact hashes. `NotSigned`, `UnknownError`, an untrusted self-signed
certificate, or a failed updater verification is a hard stop.

## 5. Publish immutable release assets

1. Generate metadata and the source archive:

   ```powershell
   .\scripts\generate-release-metadata.ps1 -Version 0.1.0 -OutputDir release-metadata
   git archive --format=zip --output release-metadata/Bard_0.1.0_source.zip HEAD
   ```

2. Upload only the signed NSIS installer, executable, `.sig`, hashes,
   provenance, SBOM, and source archive to the `miisodev/bard-releases` release
   for the exact tag.
3. Set `BARD_UPDATE_MANIFEST_URL` to the immutable `latest.json` URL and
   `BARD_WINDOWS_INSTALLER_URL` to the signed installer URL.
4. Set the Polar product's downloadable benefit to the exact signed file.
   Never label an unsigned file as signed.

## 6. Commerce and site switch

1. Run a real smallest-value purchase in Polar production mode.
2. Verify checkout price/currency, receipt email, webhook delivery, licence
   issuance, three activations, and fourth-machine rejection.
3. Test cancellation, renewal, refund/revocation, failed renewal, and portal
   deactivation. Keep the test customer and transaction IDs in the private
   release log, not in the repository.
4. Deploy the site only after the signed URLs are configured. Confirm
   `/saas/bard`, `/api/bard/update`, and `/download/bard/windows` from an
   unauthenticated browser and an authenticated customer session.

## 7. Installed smoke test

On a clean supported Windows 10/11 machine:

- install, repair, launch, complete the four-step onboarding, and start the
  local trial;
- activate the test key, go offline, restart, and confirm the signed lease
  remains valid for its advertised 30-day maximum;
- capture text and an image, dictate real speech, use Draw and Settings, and
  verify keyboard-only navigation;
- test `Shift+Shift`, `Ctrl+Ctrl`, `Ctrl+,`, `Ctrl+Shift+D`, and `Ctrl+/`;
- verify Send to Bard cold/warm/multi-file/duplicate/unreadable intake;
- check the update flow, deliberate Task Manager termination, restart, and
  uninstall cleanup;
- record screenshots at 100% and 150% scaling in light and dark themes.

## 8. Rollback

If a release fails before customer distribution, keep the previous signed
release selected in the updater manifest and remove the bad tag/assets from
the release channel. If it fails after distribution:

1. set the updater manifest back to the last known-good `latest.json`;
2. disable the new Polar product checkout and preserve the entitlement broker;
3. publish a hotfix only after the same source, signing, updater, and installed
   gates pass;
4. record affected versions, hashes, timestamps, and customer impact in the
   incident log.

Never roll back by replacing a release asset under the same version. Every
immutable release gets a new version and tag.
