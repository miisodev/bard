# Bard release runbook

Bard ships through two Windows channels, one package each:

1. **Direct download from miiso.dev** — the signed NSIS installer, which updates
   itself through the Tauri updater feed.
2. **Microsoft Store** — the MSIX package, submitted under the Partner Center
   identity. Microsoft signs, distributes and updates that copy.

There is no downloadable MSIX. A sideloaded package would carry a different
identity from the Store copy, install beside it, and never receive an update.

The release is **NO-GO** until the direct installer passes Authenticode, the
updater signature, commerce, and installed-machine gates. An unsigned local
build is for testing only and is never uploaded or labelled as a release.

## 1. The certificate

Publicly trusted code-signing keys have been hardware-bound since June 2023
(CA/Browser Forum baseline requirements): a certificate arrives on a **USB
token** or behind a **CA cloud-signing service**, never as a PFX file. As an
individual, the certificate is issued in your own name (individual
validation); organisation validation needs a registered company.

Once the token's minidriver or the cloud signer's key storage provider is
installed, the certificate appears in the Windows certificate store. Note its
SHA-1 thumbprint:

```powershell
Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Format-List Subject, Thumbprint, NotAfter
```

Signing names the publisher; it does not by itself clear SmartScreen, which
still weighs download reputation.

## 2. Freeze and gate the source

```powershell
git status --short          # must be empty
bun install --frozen-lockfile
bun run build
bun audit
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
git tag -a v0.1.0 -m "Bard 0.1.0"
```

A green build is not a release approval. Keep the output with the release record.

## 3. Build the signed direct release

```powershell
.\scripts\build-release.ps1 -CertificateThumbprint <thumbprint>
# or, for a cloud signer without store integration (%1 is the file):
.\scripts\build-release.ps1 -SignCommand "<signer> sign ... %1"
```

The script refuses a dirty worktree, passes the certificate to Tauri, and
builds once. Tauri signs `bard.exe`, the NSIS installer and its uninstaller
during the build, **then** writes the updater signature over the final
installer. Signing after the build is wrong twice over: the installer would
carry the unsigned `bard.exe`, and the updater signature would no longer match
the file. The script ends by running:

```powershell
.\scripts\verify-release.ps1 -Version 0.1.0 -RequireSignature
```

which checks NSIS integrity, Authenticode on the executable and the installer,
the updater signature envelope, the embedded updater public key, and hashes.
`NotSigned`, `UnknownError`, an untrusted certificate or a failed updater
verification is a hard stop.

The updater key comes from `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, or from `$HOME\.tauri\bard-updater.key`
and `bard-updater-password.txt`. It never enters the repository.

The CI release workflow can sign only through a cloud signer
(`WINDOWS_SIGN_COMMAND` secret); a token-held certificate signs on its own
machine with the script above. Until a certificate exists the direct release
is built here with `.\scripts\build-release.ps1 -Unsigned` (decision
2026-09-24), and a `v*` tag push starts the CI workflow, which cannot sign —
cancel that run.

## 4. Build the Store package

```powershell
.\scripts\build-msix.ps1 -Store `
  -PackageName "miisodev.BardCaptureDictation" `
  -Publisher "CN=61F791B1-D87E-4C90-9617-536200345DA6" `
  -PublisherDisplayName "miisodev" `
  -DisplayName "Bard: Capture & Dictation"
```

The values come from **Partner Center › Product identity**. The package embeds
`bard.exe` from step 3 and declares a startup task for login start. It is not
signed here: Microsoft signs it after certification. Follow
`docs/STORE_SUBMISSION.md` for the submission itself.

## 5. Publish the direct release

1. Stage the installer, a stable-name copy `Bard_x64-setup.exe`, the `.sig`,
   `latest.json` (the updater manifest, pointing at the versioned installer),
   `SHA256SUMS`, provenance and SBOM from
   `.\scripts\generate-release-metadata.ps1`.
2. Create the release for the tag on the public `miisodev/bard-releases` and
   mark it latest. **Never attach a source archive** — that repository is
   public and the source is not.
3. The site needs no change per release: `BARD_WINDOWS_INSTALLER_URL` and
   `BARD_UPDATE_MANIFEST_URL` point at `releases/latest/download/`, so the
   newest release is what `/download/bard/windows` and `/api/bard/update`
   serve. `NEXT_PUBLIC_BARD_STORE_URL` is set once the Store listing is live.
4. Upload the same installer to the Polar downloadable benefit
   (`a934d3ed-…`), replacing the previous file.

## 6. Commerce and site switch

1. Run a real smallest-value purchase in Polar production mode.
2. Verify price and currency, receipt email, webhook delivery, licence
   issuance, three activations, and fourth-machine rejection.
3. Test cancellation, renewal, refund/revocation, failed renewal, and portal
   deactivation. Keep customer and transaction IDs in the private release log.
4. Deploy the site only after the installer URL and the Store listing are
   verified. Confirm `/tools/bard`, `/api/bard/update` and
   `/download/bard/windows` from an unauthenticated browser and a customer
   session.

## 7. Installed smoke test

On a clean supported Windows 10/11 machine:

- install the NSIS package; check the per-user `Send to Bard` verb (Windows 11
  lists it under **Show more options**);
- install the Store copy on a second clean profile; check Start menu, launch,
  **Settings › Apps › Startup**, and uninstall;
- complete the four-step onboarding and start the trial;
- activate the test key, go offline, restart, and confirm the signed lease
  holds for its 30-day maximum;
- capture text and an image, dictate real speech, open Draw and Settings, and
  navigate by keyboard only;
- test `Left Shift × 2`, `Ctrl × 2`, `Right Shift × 2`, `Right Shift × 2` + `1/2/3`, `Right Alt × 2`, `Left Alt × 2`, and Draw `F11`
  and `Ctrl+/`;
- test Send to Bard cold, warm, with several files, duplicates and an
  unreadable file;
- check the update flow (direct copy), deliberate Task Manager termination,
  restart, and uninstall cleanup;
- record screenshots at 100% and 150% scaling in light and dark.

## 8. Rollback

If a release fails before customers have it, keep the previous signed release
in the updater manifest and remove the bad tag and assets. If it fails after:

1. point the updater manifest back at the last known-good `latest.json`;
2. disable checkout and keep the entitlement broker running;
3. halt or supersede the Store submission according to Partner Center policy;
4. publish a fixed version only after the same source, signing, updater, Store
   and installed gates pass;
5. record affected versions, hashes, timestamps and customer impact.

Never replace a release asset under the same version. Every release gets a new
version and tag.
