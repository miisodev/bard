# Security policy

## Reporting a vulnerability

Bard is a local desktop application: it has no server, no account and no
telemetry. A security issue in Bard is therefore almost always a problem on
one machine — a path that escapes its folder, a licence check that can be
bypassed, an update that can be forged — rather than a breach of a service
that holds anyone's data.

Report privately through the [miiso.dev support route](https://miiso.dev/contact)
rather than a public issue. Include the version (Settings → About), the
platform, what you did, and what you expected. A diagnostic report
(Settings → Data → Copy diagnostic report) carries the versions, the storage
state and the lifecycle log without leaving the machine; attaching it saves a
round trip.

Please do not open a public issue for an unfixed vulnerability before there is
a fix to reference.

## What is already in place

- The Windows installer is deliberately **not** Authenticode-signed. It does
  carry an Ed25519 updater signature, and Bard verifies every update against
  the public key built into the app before installing it.
- The Microsoft Store copy is signed by Microsoft.
- Downloads are published with `SHA256SUMS`, a software bill of materials and
  a provenance document naming the exact source commit.
- The licence key, the signed lease and any OpenRouter key live only in the
  operating system's credential store — never in the database, the logs, the
  events or a crash report.
- Refine sends the finished transcript text only, and only when switched on.
  Audio, captures and history never leave the machine.
