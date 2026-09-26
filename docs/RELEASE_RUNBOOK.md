# Bard release runbook

Bard ships through two Windows channels, one package each:

1. **Direct download from miiso.dev** — the NSIS installer, which updates itself
   through the Tauri updater feed.
2. **Microsoft Store** — the MSIX package, submitted under the Partner Center
   identity. Microsoft signs, distributes and updates that copy.

There is no downloadable MSIX. A sideloaded package would carry a different
identity from the Store copy, install beside it, and never receive an update.

The direct release is **NO-GO** until the updater signature, commerce, and
installed-machine gates pass. It is also **unsigned by decision**: no
Authenticode certificate is bought, so the direct installer ships unsigned and
Windows shows its SmartScreen notice on first run. Steps 1 and 3 below describe
the signing path for anyone who later holds a certificate; until then run
`.\scripts\build-release.ps1 -Unsigned` and skip the Authenticode check in
`verify-release.ps1`.

## 1. The certificate

No certificate is bought today, and step 3 below is the path for anyone who
later holds one.

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

## 3. Build the direct release

Until a certificate exists, build it unsigned:

```powershell
.\scripts\build-release.ps1 -Unsigned
```

With one, the signed path is:

```powershell
.\scripts\build-release.ps1 -CertificateThumbprint <thumbprint>
# or, for a cloud signer without store integration (%1 is the file):
.\scripts\build-release.ps1 -SignCommand "<signer> sign ... %1"
```

The script refuses a dirty worktree, builds once, and then signs — in two
separate steps, because the two kinds of signature are not the same problem.

**Authenticode stays in the build.** Tauri signs `bard.exe`, the NSIS installer
and its uninstaller while the build runs. This cannot be moved afterwards: the
installer is assembled *around* the already-signed `bard.exe`, so signing later
would leave the shipped installer carrying an unsigned binary. The certificate
thumbprint, or a cloud signer's key reference, is what the build carries — a
store thumbprint is a public identifier, and a cloud signer's key never enters
the build's environment.

**The updater signature is a separate step.** It used to be written by the build
itself, which meant `TAURI_SIGNING_PRIVATE_KEY` was exported into the
environment of `bun`, of `@tauri-apps/cli` inside `node_modules`, of every
lifecycle script, of `cargo`, and of every `build.rs` in the dependency graph.
One compromised transitive npm dependency was therefore a silent, total
compromise of the update chain for every installed client, with a genuinely
valid signature. The build now runs with `createUpdaterArtifacts: false` and
those three variables scrubbed from its environment, and `scripts/tauri-sign.mjs`
signs the finished installer afterwards.

That helper is not cosmetic either: `bun run tauri signer sign` still loads
JavaScript out of `node_modules`, so the helper dlopens the platform's **native**
signing binding directly. The key is passed as a file path, the password via
environment variable, never on a command line, and the script refuses to run at
all if `TAURI_SIGNING_PRIVATE_KEY` is present in its environment. The signature
is computed over the final bytes, so the artefact is not mutated by signing it.

The key itself comes from `$HOME\.tauri\bard-updater.key` and
`bard-updater-password.txt`, or from the environment on a machine that sets it.
It never enters the repository.

The script ends by running:

```powershell
.\scripts\verify-release.ps1 -Version 0.1.0 -RequireSignature
```

which checks NSIS integrity, Authenticode on the executable and the installer,
the updater signature envelope, the embedded updater public key, and hashes.
`NotSigned`, `UnknownError`, an untrusted certificate or a failed updater
verification is a hard stop. On the unsigned path drop `-RequireSignature`: the
Authenticode block is what it gates, while the Tauri updater signature is
verified either way.

The CI release workflow can sign only through a cloud signer
(`WINDOWS_SIGN_COMMAND` secret); a token-held certificate signs on its own
machine with the script above. Until a certificate exists the direct release
is built here with `.\scripts\build-release.ps1 -Unsigned` (decision
2026-09-24).

**Configure the `release` environment before publishing.** The workflow is wired
to a GitHub environment named `release`, but required reviewers are a
repository setting that YAML cannot set. Without it a `v*` tag push still
reaches a signing run unattended — and the previous runbook's answer to that was
"cancel that run", which is a procedure rather than a control.

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
   issuance, that the key arrives by email, and that it carries a 365-day term
   counted from that activation.
3. Test the three-activation limit, a fourth machine, refund/revocation,
   releasing a machine in the customer portal, and a replacement key. There is
   nothing to cancel and no failed renewal to test. Keep customer and
   transaction IDs in the private release log.
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

If a release fails before customers have it, keep the previous known-good
release in the updater manifest and remove the bad tag and assets. If it fails
after:

1. point the updater manifest back at the last known-good `latest.json`;
2. disable checkout and keep the entitlement broker running;
3. halt or supersede the Store submission according to Partner Center policy;
4. publish a fixed version only after the same source, updater-signature, Store
   and installed gates pass;
5. record affected versions, hashes, timestamps and customer impact.

Never replace a release asset under the same version. Every release gets a new
version and tag.
