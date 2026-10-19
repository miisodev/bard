/*
  generate-release-metadata.mjs — write a release's provenance and SBOM.

  scripts/generate-release-metadata.ps1 is the Windows release's generator and
  stays so. This file is the Linux release's counterpart, and it is a Node
  sibling rather than a switch on the PowerShell script because the Linux
  release box runs bash, bun and node — not PowerShell. It reads the same
  inputs and keeps the same two document shapes:

    release-provenance.json — schema string, product, version, git commit,
      generated_at, the artifact list (basename, bytes, sha256) and a signing
      block;
    bard-sbom.json — CycloneDX 1.5: the real dependency inventory, one library
      component per crate in the platform's Cargo resolution (name, version,
      purl, licence) and per production npm dependency (name, version, purl,
      licence), plus the npm application component.

  Only the signing block differs, and it states the Linux truth: no
  Authenticode, no Microsoft Store package, and no Tauri updater artifact — a
  deb or rpm is updated by the package manager, and Flathub updates the
  flatpak.

  Run it from the repository root after the release build:

    bun scripts/generate-release-metadata.mjs --platform linux \
      [--version <v>] [--bundle-dir <dir>] [--output-dir <dir>] [--target <triple>]

  --platform defaults to the host (windows on Windows, linux elsewhere).
  --bundle-dir defaults to the NSIS output folder on Windows and to the bundle
  root on Linux; --output-dir defaults to release-metadata, as the PowerShell
  script's does. --target defaults to the platform's release triple
  (x86_64-pc-windows-msvc or x86_64-unknown-linux-gnu) and only scopes the
  Cargo resolution — no build happens here, so the triple need not be
  installed.

  The dependency inventory needs a warm frontend install: run
  `bun install --frozen-lockfile` first, as the release order does.
*/

import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function fail(message) {
  console.error(`generate-release-metadata: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) fail(`unexpected argument ${arg}`);
    const name = arg.slice(2);
    const value = argv[i + 1];
    if (value === undefined || value.startsWith("--")) fail(`--${name} needs a value`);
    options[name] = value;
    i += 1;
  }
  return options;
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex").toUpperCase();
}

/** Run a required tool from the repository root, failing with its own words. */
function run(command, args) {
  try {
    // Cargo metadata for the resolved graph is tens of megabytes of JSON, and
    // execFileSync's default maxBuffer is 1 MiB.
    return execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      maxBuffer: 256 * 1024 * 1024,
    });
  } catch (error) {
    if (error.code === "ENOENT") fail(`${command} is not on PATH.`);
    const detail = (error.stderr ?? "").trim() || error.message;
    fail(`${command} failed: ${detail}`);
  }
}

function artifact(path) {
  // statSync throws on a missing artifact, exactly as Get-Item does in the
  // PowerShell script: metadata that describes a file that is not there is
  // worse than no metadata.
  const item = statSync(path);
  return { file: basename(path), bytes: item.size, sha256: sha256(path) };
}

/** The single artifact of one extension in a bundle folder. */
function theArtifact(dir, extension) {
  if (!existsSync(dir)) fail(`no ${extension} folder at ${dir}. Build the release first.`);
  const found = readdirSync(dir)
    .filter((name) => name.endsWith(extension))
    .map((name) => join(dir, name));
  if (found.length === 0) fail(`no ${extension} artifact in ${dir}.`);
  if (found.length > 1) {
    const names = found.map((path) => basename(path)).join(", ");
    fail(`more than one ${extension} in ${dir}: ${names}. Clear stale bundles first.`);
  }
  return artifact(found[0]);
}

const options = parseArgs(process.argv.slice(2));
const platform = options.platform ?? (process.platform === "win32" ? "windows" : "linux");
if (platform !== "windows" && platform !== "linux") {
  fail(`--platform must be windows or linux, not ${platform}`);
}

const version =
  options.version ?? JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8")).version;
const bundleDir = resolve(
  options["bundle-dir"] ??
    join(repoRoot, "src-tauri/target/release/bundle", platform === "windows" ? "nsis" : ""),
);
const outputDir = resolve(options["output-dir"] ?? join(repoRoot, "release-metadata"));

const commit = run("git", ["rev-parse", "HEAD"]).trim();

let artifacts;
let signing;
if (platform === "windows") {
  const releaseDir = join(repoRoot, "src-tauri/target/release");
  const installer = join(bundleDir, `Bard_${version}_x64-setup.exe`);
  artifacts = [
    artifact(join(releaseDir, "bard.exe")),
    artifact(installer),
    artifact(`${installer}.sig`),
  ];
  signing = {
    authenticode: { required: false, status: "unsigned by decision; SmartScreen shows its notice once" },
    microsoft_store: {
      public_asset: false,
      status: "MSIX submitted through Partner Center; Microsoft signs it after certification",
    },
    tauri_updater: { required: true, envelope: `${installer}.sig` },
  };
} else {
  artifacts = [
    theArtifact(join(bundleDir, "deb"), ".deb"),
    theArtifact(join(bundleDir, "rpm"), ".rpm"),
  ];
  signing = {
    authenticode: { required: false, status: "not applicable; Authenticode signs Windows binaries only" },
    microsoft_store: { public_asset: false, status: "not applicable; the MSIX is a Windows package" },
    tauri_updater: {
      required: false,
      status: "no updater artifact for deb or rpm; Flathub updates the flatpak",
    },
  };
}

const provenance = {
  schema: "https://miiso.dev/schemas/bard-release-provenance-1.json",
  product: "Bard",
  version,
  git_commit: commit,
  generated_at: new Date().toISOString(),
  source_archive: "Not generated; the source is private and Bard releases carry no source archive.",
  artifacts,
  signing,
};

/**
 * CycloneDX 1.5 takes either a licence object or an SPDX expression. Most
 * crates carry an SPDX expression; the older "MIT/Apache-2.0" spelling is a
 * cargo convention, not SPDX, so it travels as a named licence instead.
 */
function licenseChoice(text) {
  const value = typeof text === "string" ? text.trim() : "";
  if (value === "") return undefined;
  const spdx = value
    .split(/\s+/)
    .every((token) => /^(?:\(*[A-Za-z0-9.+-]+\)*|OR|AND|WITH)$/.test(token));
  return spdx ? { expression: value } : { license: { name: value } };
}

/** npm allows a string, a { type, url } object, or an array of either. */
function licenseChoices(value) {
  if (typeof value === "string") {
    const choice = licenseChoice(value);
    return choice ? [choice] : undefined;
  }
  if (Array.isArray(value)) {
    const texts = value
      .map((entry) => (typeof entry === "string" ? entry : entry?.type))
      .filter((entry) => typeof entry === "string" && entry.trim() !== "");
    if (texts.length === 0) return undefined;
    const choice = licenseChoice(texts.join(" OR "));
    return choice ? [choice] : undefined;
  }
  if (value && typeof value === "object" && typeof value.type === "string") {
    const choice = licenseChoice(value.type);
    return choice ? [choice] : undefined;
  }
  return undefined;
}

function withLicense(component, choices) {
  return choices ? { ...component, licenses: choices } : component;
}

const target =
  options.target ??
  (platform === "windows" ? "x86_64-pc-windows-msvc" : "x86_64-unknown-linux-gnu");

// The whole resolved graph for this platform — not --no-deps: an SBOM whose
// component list is the product itself cannot answer what it exists to answer.
// --filter-platform scopes it to the crates this platform actually compiles
// (no GTK/WebKit on Windows), and resolves without building anything.
const cargo = JSON.parse(
  run("cargo", [
    "metadata",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--locked",
    "--format-version",
    "1",
    "--filter-platform",
    target,
  ]),
);
const crates = cargo.packages
  .map((entry) =>
    withLicense(
      {
        type: "library",
        name: entry.name,
        version: entry.version,
        purl: `pkg:cargo/${entry.name}@${entry.version}`,
      },
      licenseChoices(entry.license),
    ),
  )
  .sort((a, b) => a.purl.localeCompare(b.purl));

const application = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8"));
if (!existsSync(join(repoRoot, "node_modules"))) {
  fail(
    "node_modules is missing; the SBOM needs the installed frontend dependencies. Run bun install --frozen-lockfile first.",
  );
}
const npm = Object.keys(application.dependencies ?? {})
  .sort()
  .map((name) => {
    const installed = join(repoRoot, "node_modules", ...name.split("/"), "package.json");
    if (!existsSync(installed)) {
      fail(`node_modules/${name} is missing; run bun install --frozen-lockfile first.`);
    }
    const entry = JSON.parse(readFileSync(installed, "utf8"));
    return withLicense(
      {
        type: "library",
        name,
        version: entry.version,
        purl: `pkg:npm/${name}@${entry.version}`,
      },
      licenseChoices(entry.license ?? entry.licenses),
    );
  });

const components = [
  ...crates,
  ...npm,
  {
    type: "application",
    name: application.name,
    version: application.version,
    purl: `pkg:npm/${application.name}@${application.version}`,
  },
];
const sbom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  serialNumber: `urn:uuid:${randomUUID()}`,
  version: 1,
  metadata: { component: { type: "application", name: "Bard", version } },
  components,
};

mkdirSync(outputDir, { recursive: true });
for (const [name, document] of [
  ["release-provenance.json", provenance],
  ["bard-sbom.json", sbom],
]) {
  const path = join(outputDir, name);
  writeFileSync(path, `${JSON.stringify(document, null, 2)}\n`);
  console.log(`metadata  ${path}  (${statSync(path).size} bytes)`);
}
console.log(`platform  ${platform}`);
for (const entry of artifacts) {
  console.log(`artifact  ${entry.file}  ${entry.bytes} bytes  sha256 ${entry.sha256}`);
}
