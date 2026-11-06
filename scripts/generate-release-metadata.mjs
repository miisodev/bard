/*
  generate-release-metadata.mjs — write a release's provenance and SBOM.

  scripts/generate-release-metadata.ps1 is the Windows release's generator and
  stays so. This file is the Linux release's counterpart, and it is a Node
  sibling rather than a switch on the PowerShell script because the Linux
  release box runs bash, bun and node — not PowerShell. It reads the same
  inputs and keeps the same two document shapes:

    release-provenance.json — schema string, product, version, source commit,
      generated_at, the artifact list (basename, bytes, sha256) and a signing
      block;
    bard-sbom.json — CycloneDX 1.5: the real dependency inventory, one library
      component per crate reachable in the platform's resolved Cargo graph and
      per package in the installed production npm closure (name, version,
      purl, licence), plus the npm application component.

  Only the signing block differs, and it states the truth: no Authenticode;
  the Microsoft Store MSIX is prepared for owner submission and Microsoft signs
  it after certification; deb/rpm have no Tauri updater artifact because the
  package manager updates them. No Flathub package is produced by this release.

  Run it from the repository root after the release build:

    bun scripts/generate-release-metadata.mjs --platform linux \
      [--version <v>] [--bundle-dir <dir>] [--binary-dir <dir>] \
      [--output-dir <dir>] [--target <triple>] [--source-commit <sha>]

  --platform defaults to the host (windows on Windows, linux elsewhere).
  --bundle-dir defaults to the NSIS output folder on Windows and to the bundle
  root on Linux; --output-dir defaults to release-metadata, as the PowerShell
  script's does. --binary-dir points at the hosted build's executable directory
  on Windows (and defaults --bundle-dir to <binary-dir>/bundle/nsis); on Linux
  it defaults --bundle-dir when that is omitted, because the deb/rpm packages
  are the release assets. --target defaults to the platform's release triple
  (x86_64-pc-windows-msvc or x86_64-unknown-linux-gnu) and only scopes the
  Cargo resolution — no build happens here, so the triple need not be
  installed.

  --source-commit is for hosted-built packages signed locally. It must be a
  full 40-character commit that exists, and its product source and version must
  match the current tree. Without it the default remains HEAD, as before.

  The dependency inventory needs a warm frontend install: run
  `bun install --frozen-lockfile` first, as the release order does.
*/

import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  PRODUCT_SOURCE_PATHS,
  collectCargoClosure,
  collectNpmClosure,
  createFsPackageReader,
  isFullCommitSha,
  npmPurl,
  parseCargoLockPackage,
  parseCargoPackageIdentity,
  validateSourceIdentity,
} from "./release-metadata-lib.mjs";

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
  // Metadata that describes a file that is not there is worse than no
  // metadata, so a missing release asset is a clear failure.
  if (!existsSync(path)) fail(`missing release artifact: ${path}`);
  const item = statSync(path);
  if (!item.isFile()) fail(`release artifact is not a file: ${path}`);
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

function readIdentity(readText) {
  return {
    package: JSON.parse(readText("package.json")),
    tauri: JSON.parse(readText("src-tauri/tauri.conf.json")),
    cargo: parseCargoPackageIdentity(readText("src-tauri/Cargo.toml")),
    cargoLock: parseCargoLockPackage(readText("src-tauri/Cargo.lock"), "bard"),
  };
}

function resolveSourceCommit(requested) {
  if (!isFullCommitSha(requested)) {
    fail("--source-commit must be a full 40-character hexadecimal commit SHA.");
  }
  let resolved;
  try {
    resolved = execFileSync("git", ["rev-parse", "--verify", `${requested}^{commit}`], {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  } catch (error) {
    const detail = (error.stderr ?? "").trim() || error.message;
    fail(`--source-commit ${requested} does not name a commit: ${detail}`);
  }
  if (!isFullCommitSha(resolved) || resolved.toLowerCase() !== requested.toLowerCase()) {
    fail(`--source-commit ${requested} resolved to ${resolved}, not the full commit.`);
  }
  try {
    execFileSync("git", ["cat-file", "-e", `${resolved}^{commit}`], {
      cwd: repoRoot,
      stdio: "ignore",
    });
  } catch {
    fail(`--source-commit ${requested} does not exist in this repository.`);
  }
  return resolved;
}

/**
 * A hosted build may be signed from a later local commit, but only when the
 * product source in the claimed commit is exactly what this tree builds. A
 * release script change is fine; an app, Cargo or lockfile difference is not.
 */
function productSourceMatches(commit) {
  try {
    execFileSync("git", ["diff", "--quiet", commit, "--", ...PRODUCT_SOURCE_PATHS], {
      cwd: repoRoot,
      stdio: "ignore",
    });
  } catch (error) {
    if (error.status === 1) return false;
    const detail = (error.stderr ?? "").trim() || error.message;
    fail(`could not compare product source with ${commit}: ${detail}`);
  }
  let untracked;
  try {
    untracked = execFileSync(
      "git",
      ["ls-files", "--others", "--exclude-standard", "--", ...PRODUCT_SOURCE_PATHS],
      { cwd: repoRoot, encoding: "utf8" },
    ).trim();
  } catch (error) {
    const detail = (error.stderr ?? "").trim() || error.message;
    fail(`could not inspect untracked product source: ${detail}`);
  }
  return untracked === "";
}

const options = parseArgs(process.argv.slice(2));
const platform = options.platform ?? (process.platform === "win32" ? "windows" : "linux");
if (platform !== "windows" && platform !== "linux") {
  fail(`--platform must be windows or linux, not ${platform}`);
}

const currentPackage = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8"));
const version = options.version ?? currentPackage.version;
const defaultBundleDir = join(
  repoRoot,
  "src-tauri/target/release/bundle",
  platform === "windows" ? "nsis" : "",
);
const binaryDir = resolve(
  options["binary-dir"] ??
    (platform === "windows"
      ? join(repoRoot, "src-tauri/target/release")
      : resolve(options["bundle-dir"] ?? defaultBundleDir)),
);
const bundleDir = resolve(
  options["bundle-dir"] ??
    (options["binary-dir"]
      ? platform === "windows"
        ? join(binaryDir, "bundle/nsis")
        : binaryDir
      : defaultBundleDir),
);
const outputDir = resolve(options["output-dir"] ?? join(repoRoot, "release-metadata"));

let commit;
if (options["source-commit"] !== undefined) {
  commit = resolveSourceCommit(options["source-commit"]);
  const source = readIdentity((path) => run("git", ["show", `${commit}:${path}`]));
  const current = readIdentity((path) => readFileSync(join(repoRoot, path), "utf8"));
  const errors = validateSourceIdentity({
    source,
    current,
    version: options.version,
  });
  if (errors.length > 0) {
    fail(`--source-commit does not describe this release:\n- ${errors.join("\n- ")}`);
  }
  if (!productSourceMatches(commit)) {
    fail(
      `--source-commit ${commit} has product source that differs from the current tree; check out the hosted build's source or sign the matching build.`,
    );
  }
} else {
  commit = run("git", ["rev-parse", "HEAD"]).trim();
}

let artifacts;
let signing;
if (platform === "windows") {
  const installer = join(bundleDir, `Bard_${version}_x64-setup.exe`);
  artifacts = [
    artifact(join(binaryDir, "bard.exe")),
    artifact(installer),
    artifact(`${installer}.sig`),
  ];
  signing = {
    authenticode: {
      required: false,
      status: "unsigned by decision; SmartScreen shows its notice once",
    },
    microsoft_store: {
      public_asset: false,
      status:
        "MSIX package prepared for owner submission through Partner Center; not yet submitted. Microsoft signs it after certification.",
    },
    tauri_updater: { required: true, envelope: `${installer}.sig` },
  };
} else {
  artifacts = [
    theArtifact(join(bundleDir, "deb"), ".deb"),
    theArtifact(join(bundleDir, "rpm"), ".rpm"),
  ];
  signing = {
    authenticode: {
      required: false,
      status: "not applicable; Authenticode signs Windows binaries only",
    },
    microsoft_store: {
      public_asset: false,
      status: "not applicable; the MSIX is a Windows package",
    },
    tauri_updater: {
      required: false,
      status:
        "no updater artifact for deb or rpm; the package manager updates them. No Flathub package is produced by this release.",
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

const target =
  options.target ??
  (platform === "windows" ? "x86_64-pc-windows-msvc" : "x86_64-unknown-linux-gnu");

// `cargo metadata --filter-platform` leaves `packages` unfiltered but filters
// resolve.nodes[].deps. The closure walker treats those nodes as the authority,
// so target-only crates never leak into the SBOM and dev-only edges are skipped.
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

let crates;
try {
  crates = collectCargoClosure(cargo);
} catch (error) {
  fail(error.message);
}

const application = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8"));
if (!existsSync(join(repoRoot, "node_modules"))) {
  fail(
    "node_modules is missing; the SBOM needs the installed frontend dependencies. Run bun install --frozen-lockfile first.",
  );
}

let npm;
try {
  npm = collectNpmClosure({
    rootDir: repoRoot,
    dependencies: Object.keys(application.dependencies ?? {}),
    optionalDependencies: Object.keys(application.optionalDependencies ?? {}),
    peerDependencies: Object.keys(application.peerDependencies ?? {}),
    peerDependenciesMeta: application.peerDependenciesMeta ?? {},
    readPackage: createFsPackageReader(repoRoot),
  });
} catch (error) {
  fail(error.message);
}

const applicationComponent = {
  type: "application",
  name: application.name,
  version,
  purl: npmPurl(application.name, version),
};
const components = [...crates, ...npm, applicationComponent].sort((a, b) =>
  a.purl.localeCompare(b.purl),
);
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
console.log(`source    ${commit}`);
for (const entry of artifacts) {
  console.log(`artifact  ${entry.file}  ${entry.bytes} bytes  sha256 ${entry.sha256}`);
}
