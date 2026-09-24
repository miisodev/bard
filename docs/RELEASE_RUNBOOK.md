# Bard release runbook

This runbook is for an operator releasing Bard through both supported Windows
channels:

1. **Microsoft Store** — submit an MSIX package; Microsoft re-signs the Store
   copy after certification.
2. **Direct download from miiso.dev** — publish the same release as a signed
   NSIS installer and a signed MSIX package.

The release is **NO-GO** until the direct artifacts pass Authenticode, the
updater signature, package validation, commerce, and installed-machine gates.
An unsigned local package is useful for testing only and must never be uploaded
to the public release repository or labelled as a signed release.

## 1. Freeze

```powershell
git status --short
git rev-parse HEAD
git tag -a v0.1.0 <commit> -m "Bard 0.1.0"
```

Do not build a release from a dirty worktree. The commit, source archive,
`Cargo.lock`, frontend lockfile, NSIS package, and MSIX package must describe
the same tree.

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

## 3. Build NSIS and the updater signature

The updater private key is supplied through `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; it must never be committed.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$HOME\.tauri\bard-updater.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content "$HOME\.tauri\bard-updater-password.txt" -Raw).Trim()
$env:BARD_NO_GUARDIAN = "1"
bun run tauri build --bundles nsis
```

The NSIS output is:

```text
src-tauri/target/release/bundle/nsis/Bard_0.1.0_x64-setup.exe
```

The app executable is:

```text
src-tauri/target/release/bard.exe
```

MSI remains retired. MSIX is created separately because Tauri's Windows
bundler currently emits NSIS/MSI rather than an MSIX package.

## 4. Build and sign the MSIX package

`build-msix.ps1` creates a desktop full-trust MSIX from the same `bard.exe` and
copies the Store assets into the package. Without a certificate it creates an
unsigned package for local manifest/package validation only.

```powershell
# Local package validation only; not publishable.
bun run build:msix

# Public direct-download build. The PFX is supplied outside the repository.
.\scripts\build-msix.ps1 `
  -Version 0.1.0 `
  -PackageName "<Partner Center identity name>" `
  -Publisher "CN=<exact code-signing certificate subject>" `
  -CertificatePath "$HOME\.secrets\bard-code-signing.pfx" `
  -RequireSignature
```

The script uses the Windows SDK `makeappx.exe`, validates the manifest during
packing, unpacks the result as a structural check, signs `bard.exe` before
packing, then signs the outer MSIX package. The exact publisher string must
match the certificate subject used for signing.

For a normal release flow where `sign-release.ps1` has already signed the
executable, use `-SkipBinarySigning` so the package embeds that exact signed
binary:

```powershell
.\scripts\build-msix.ps1 `
  -Version 0.1.0 `
  -PackageName "<Partner Center identity name>" `
  -Publisher "CN=<exact code-signing certificate subject>" `
  -CertificatePath "$HOME\.secrets\bard-code-signing.pfx" `
  -RequireSignature `
  -SkipBinarySigning
```

The public trust certificate is deliberately external to this repository. The
current Azure Artifact Signing account is an individual South African profile;
Microsoft's current public-trust eligibility rules do not make that profile a
suitable substitute for a publicly trusted direct-download certificate. Do not
alter identity or geography to work around validation.

## 5. Sign the executable and NSIS installer

Obtain the publisher certificate through the approved commercial signing
provider. Keep the PFX and password outside the repository and CI logs.

```powershell
.\scripts\sign-release.ps1 `
  -Version 0.1.0 `
  -CertificatePath "$HOME\.secrets\bard-code-signing.pfx"
.\scripts\verify-release.ps1 `
  -Version 0.1.0 `
  -RequireSignature `
  -RequireMsix
```

The verifier checks NSIS archive integrity, Authenticode status for the
executable and installer, the MSIX package signature when `-RequireMsix` is
set, the Tauri/minisign updater signature, the embedded updater public key, and
artifact hashes. `NotSigned`, `UnknownError`, an untrusted self-signed
certificate, a missing MSIX, or a failed updater verification is a hard stop.

## 6. Publish immutable release assets

1. Generate metadata and the source archive:

   ```powershell
   .\scripts\generate-release-metadata.ps1 -Version 0.1.0 -OutputDir release-metadata
   git archive --format=zip --output release-metadata/Bard_0.1.0_source.zip HEAD
   ```

2. Upload only the signed NSIS installer, signed MSIX package, executable,
   `.sig`, hashes, provenance, SBOM, and source archive to the
   `miisodev/bard-releases` release for the exact tag.

3. Create the immutable updater `latest.json` with the signed NSIS URL and the
   Tauri signature. Set `BARD_UPDATE_MANIFEST_URL` to that URL.

4. Configure the direct-download environment variables:

   ```text
   BARD_WINDOWS_INSTALLER_URL=https://.../Bard_0.1.0_x64-setup.exe
   BARD_MSIX_URL=https://.../Bard_0.1.0_x64.msix
   ```

5. Replace the Polar downloadable benefit with the exact signed NSIS and MSIX
   files, or configure equivalent authenticated delivery for both formats.
   The Store copy updates through Microsoft; the current Tauri updater feed
   serves direct NSIS updates, while a directly sideloaded MSIX receives a new
   signed package until an MSIX update feed is configured.

6. Create the Microsoft Store submission from the same MSIX package. Follow
   `docs/STORE_SUBMISSION.md` for the Partner Center checklist. The Store
   listing, screenshots, privacy URL, age rating, and package identity must
   match the published product page. The Store re-signs its copy; do not submit
   the NSIS installer as a substitute for the MSIX package.

## 7. Commerce and site switch

1. Run a real smallest-value purchase in Polar production mode.
2. Verify checkout price/currency, receipt email, webhook delivery, licence
   issuance, three activations, and fourth-machine rejection.
3. Test cancellation, renewal, refund/revocation, failed renewal, and portal
   deactivation. Keep the test customer and transaction IDs in the private
   release log, not in the repository.
4. Deploy the site only after both direct-download URLs and the Store package
   are verified. Confirm `/tools/bard`, `/tools/brainllm`, `/api/bard/update`,
   `/download/bard/windows`, and `/download/bard/windows?format=msix` from an
   unauthenticated browser and an authenticated customer session.

## 8. Installed smoke test

On a clean supported Windows 10/11 machine:

- install and launch the NSIS package; verify the per-user `Send to Bard` verb;
- install and launch the MSIX package on a second clean profile; verify Start
  menu, launch, uninstall, and package identity;
- complete the four-step onboarding and start the local trial;
- activate the test key, go offline, restart, and confirm the signed lease
  remains valid for its advertised 30-day maximum;
- capture text and an image, dictate real speech, use Draw and Settings, and
  verify keyboard-only navigation;
- test `Shift+Shift`, `Ctrl+Ctrl`, `Ctrl+,`, `Ctrl+Shift+D`, and `Ctrl+/`;
- verify Send to Bard cold/warm/multi-file/duplicate/unreadable intake;
- check the update flow, deliberate Task Manager termination, restart, and
  uninstall cleanup;
- record screenshots at 100% and 150% scaling in light and dark themes.

## 9. Rollback

If a release fails before customer distribution, keep the previous signed
release selected in the updater manifest and remove the bad tag/assets from
the release channel. If it fails after distribution:

1. set the updater manifest back to the last known-good `latest.json`;
2. disable the new Polar product checkout and preserve the entitlement broker;
3. remove or supersede the Store submission according to Partner Center policy;
4. publish a hotfix only after the same source, signing, updater, Store
   package, and installed gates pass;
5. record affected versions, hashes, timestamps, and customer impact in the
   incident log.

Never roll back by replacing a release asset under the same version. Every
immutable release gets a new version and tag.
