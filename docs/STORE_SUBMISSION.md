# Bard Microsoft Store submission

Bard's Microsoft Store channel is the **MSIX** package built from the same
release commit as the direct NSIS/MSIX downloads.

## Package

Build the package from a frozen release commit:

```powershell
bun run tauri build --bundles nsis
.\scripts\build-msix.ps1 `
  -Version 0.1.0 `
  -PackageName "<exact Partner Center identity name>" `
  -Publisher "CN=<exact signing certificate subject>" `
  -CertificatePath "$HOME\.secrets\bard-code-signing.pfx" `
  -RequireSignature
```

The package is a Windows desktop full-trust package containing `Bard.exe` and
Store assets. It does not include the NSIS installer. The Windows SDK
`makeappx.exe` validates the manifest while packing; the package must also pass
Partner Center validation and the Windows App Certification Kit before release.

For the Store submission, Microsoft re-signs the MSIX package after
certification. The direct-download copy still needs the public code-signing
certificate; the Store copy and the direct copy are separate release assets.

## Partner Center checklist

- Reserve the final product name and copy the exact identity name into the
  package manifest.
- Use `miiso.dev/privacy` as the privacy-policy URL.
- Select the correct category and declare the paid $20/year licence and 14-day
  trial if offered through the Store.
- Supply at least one real Bard screenshot and the required Store logo assets;
  do not use a development-only or unsigned screenshot.
- Declare the Windows 10/11 system requirements and the WebView2/runtime
  assumptions honestly.
- Upload the validated MSIX package, not the NSIS installer, for the MSIX
  submission path.
- Complete the privacy, age-rating, pricing/availability, and submission-notes
  sections before requesting certification.
- Test the Store-installed package on a clean Windows account before release.

## Release relationship

The Store listing and `/tools/bard` must describe the same version, price,
trial, local-first boundary, privacy disclosures, and support contact. The
release record must contain:

- source commit and tag;
- NSIS and MSIX SHA-256 hashes;
- the signed MSIX package identity and version;
- Partner Center submission/certification receipt;
- clean-machine install, launch, update, and uninstall evidence.

Do not call the Store channel live until the listing is published and the
production download variables point at the matching signed direct artifacts.
