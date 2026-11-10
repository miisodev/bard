/*
  release-metadata-lib.mjs — pure helpers for generate-release-metadata.mjs.

  The CLI owns filesystem/git/cargo discovery. Everything here takes plain
  data so the dependency graph and source-identity rules can be tested without
  a build, a network call or a real keyring.
*/

import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

/**
 * Runtime/product paths that must agree with a claimed hosted source commit.
 *
 * Every page the app ships is here. `pill.html` was missing, so a build that
 * only changed the status pill could have been described by the commit it did
 * not come from; `overlay.html` was listed but no longer exists.
 */
export const PRODUCT_SOURCE_PATHS = [
  "src",
  "src-tauri",
  "package.json",
  "bun.lock",
  "index.html",
  "settings.html",
  "draw.html",
  "pill.html",
];

export function isFullCommitSha(value) {
  return typeof value === "string" && /^[0-9a-f]{40}$/i.test(value);
}

/** npm dependency names are trusted only when they cannot escape node_modules. */
export function isNpmPackageName(name) {
  if (typeof name !== "string" || name === "" || name.startsWith(".") || name.includes("\\")) {
    return false;
  }
  const parts = name.split("/");
  if (parts.length === 1) {
    return !name.startsWith("@") && parts[0] !== "." && parts[0] !== "..";
  }
  if (parts.length === 2) {
    return (
      parts[0].startsWith("@") &&
      parts[0].length > 1 &&
      parts[1] !== "" &&
      parts[1] !== "." &&
      parts[1] !== ".."
    );
  }
  return false;
}

/**
 * CycloneDX 1.5 takes either a licence object or an SPDX expression. Most
 * crates carry an SPDX expression; the older "MIT/Apache-2.0" spelling is a
 * cargo convention, not SPDX, so it travels as a named licence instead.
 */
export function licenseChoice(text) {
  const value = typeof text === "string" ? text.trim() : "";
  if (value === "") return undefined;
  const spdx = value
    .split(/\s+/)
    .every((token) => /^(?:\(*[A-Za-z0-9.+-]+\)*|OR|AND|WITH)$/.test(token));
  return spdx ? { expression: value } : { license: { name: value } };
}

/** npm allows a string, a { type, url } object, or an array of either. */
export function licenseChoices(value) {
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

export function withLicense(component, choices) {
  return choices ? { ...component, licenses: choices } : component;
}

/** npm purl keeps the scope slash but percent-encodes its leading @. */
export function npmPurl(name, version) {
  const encoded = name.startsWith("@") ? `%40${name.slice(1)}` : name;
  return `pkg:npm/${encoded}@${version}`;
}

export function cargoPurl(name, version) {
  return `pkg:cargo/${name}@${version}`;
}

/** The [package] name/version in a Cargo.toml, not a dependency version. */
export function parseCargoPackageIdentity(text) {
  const section = text.match(/^\[package\][\s\S]*?(?=^\[|$(?![\s\S]))/m)?.[0] ?? "";
  return {
    name: section.match(/^name\s*=\s*"([^"]+)"/m)?.[1],
    version: section.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
  };
}

/** The [[package]] block for one crate in Cargo.lock. */
export function parseCargoLockPackage(text, name) {
  for (const block of text.split("[[package]]").slice(1)) {
    const blockName = block.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    if (blockName === name) {
      return {
        name: blockName,
        version: block.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
      };
    }
  }
  return undefined;
}

/**
 * Compare the identity a commit claims with the tree the release is being made
 * from. Returns human-readable errors rather than throwing so the caller can
 * report every disagreement at once.
 */
export function validateSourceIdentity({ source, current, version }) {
  const errors = [];
  const expected = version ?? current.package?.version;

  const compare = (label, sourceValue, currentValue, checkExpectedVersion = false) => {
    if (sourceValue === undefined || sourceValue === null || sourceValue === "") {
      errors.push(`source commit has no ${label}`);
      return;
    }
    if (currentValue === undefined || currentValue === null || currentValue === "") {
      errors.push(`current tree has no ${label}`);
      return;
    }
    if (sourceValue !== currentValue) {
      errors.push(`${label} differs: source ${sourceValue}, current ${currentValue}`);
    }
    if (checkExpectedVersion && expected !== undefined && sourceValue !== expected) {
      errors.push(`${label} differs from requested version ${expected}: source ${sourceValue}`);
    }
  };

  compare("package name", source.package?.name, current.package?.name);
  compare("package version", source.package?.version, current.package?.version, true);
  compare("Tauri product name", source.tauri?.productName, current.tauri?.productName);
  compare("Tauri version", source.tauri?.version, current.tauri?.version, true);
  compare("Cargo package name", source.cargo?.name, current.cargo?.name);
  compare("Cargo package version", source.cargo?.version, current.cargo?.version, true);
  if (source.cargoLock || current.cargoLock) {
    compare(
      "Cargo.lock Bard version",
      source.cargoLock?.version,
      current.cargoLock?.version,
      true,
    );
  }

  return errors;
}

/**
 * Resolve an installed package the way Node does for a hoisted install: start
 * at the dependent's directory, then walk up checking node_modules at each
 * level. Scoped names are joined as two path segments. No realpath call: a
 * symlinked node_modules entry must keep its visible parent chain so a hoisted
 * sibling remains reachable.
 */
export function createFsPackageReader(rootDir) {
  const root = resolve(rootDir);
  return (name, fromDir) => {
    let current = resolve(fromDir);
    while (true) {
      const dir = join(current, "node_modules", ...name.split("/"));
      const manifest = join(dir, "package.json");
      if (existsSync(manifest)) {
        return { dir, packageJson: JSON.parse(readFileSync(manifest, "utf8")) };
      }
      const parent = dirname(current);
      if (parent === current || current === root) break;
      current = parent;
    }
    return null;
  };
}

/**
 * Walk the installed production npm closure.
 *
 * `dependencies` are required and a missing one fails. `optionalDependencies`
 * are included when installed and skipped when absent. A peer is required
 * unless `peerDependenciesMeta` marks it optional; peers are included when
 * installed. `devDependencies` are never traversed, so dev-only packages
 * cannot leak into the SBOM.
 */
export function collectNpmClosure({
  rootDir,
  dependencies = [],
  optionalDependencies = [],
  peerDependencies = [],
  peerDependenciesMeta = {},
  readPackage,
}) {
  const components = new Map();
  const visited = new Set();
  const queue = [];

  const enqueue = (name, fromDir, optional, chain) => {
    queue.push({ name, fromDir, optional, chain });
  };

  for (const name of dependencies) enqueue(name, rootDir, optionalDependencies.includes(name), "root");
  for (const name of optionalDependencies) enqueue(name, rootDir, true, "root");
  for (const name of peerDependencies) {
    enqueue(name, rootDir, peerDependenciesMeta[name]?.optional === true, "root");
  }

  while (queue.length > 0) {
    const { name, fromDir, optional, chain } = queue.shift();
    if (!isNpmPackageName(name)) {
      throw new Error(`invalid npm dependency name "${name}" required by ${chain}`);
    }
    const found = readPackage(name, fromDir);
    if (!found) {
      if (optional) continue;
      throw new Error(
        `missing required npm dependency "${name}" required by ${chain}; run bun install --frozen-lockfile first`,
      );
    }
    const pkg = found.packageJson;
    if (!pkg || typeof pkg.version !== "string" || pkg.version === "") {
      throw new Error(`npm package "${name}" at ${found.dir} has no version`);
    }
    // An npm alias can resolve a requested name to a differently named
    // installed package. The SBOM must describe the bytes that are installed.
    const componentName = typeof pkg.name === "string" && pkg.name !== "" ? pkg.name : name;
    const purl = npmPurl(componentName, pkg.version);
    // Equal package versions can have different nested/peer resolutions.
    // Deduplicate exported components while traversing each installed location.
    if (visited.has(found.dir)) continue;
    visited.add(found.dir);

    components.set(
      purl,
      withLicense(
        { type: "library", name: componentName, version: pkg.version, purl },
        licenseChoices(pkg.license ?? pkg.licenses),
      ),
    );

    const childChain = `${chain} > ${name}`;
    for (const dep of Object.keys(pkg.dependencies ?? {})) {
      enqueue(dep, found.dir, Object.hasOwn(pkg.optionalDependencies ?? {}, dep), childChain);
    }
    for (const dep of Object.keys(pkg.optionalDependencies ?? {})) {
      enqueue(dep, found.dir, true, childChain);
    }
    for (const dep of Object.keys(pkg.peerDependencies ?? {})) {
      enqueue(dep, found.dir, pkg.peerDependenciesMeta?.[dep]?.optional === true, childChain);
    }
  }

  return [...components.values()].sort((a, b) => a.purl.localeCompare(b.purl));
}

/**
 * Walk the root package's resolved Cargo dependency graph.
 *
 * `cargo metadata --filter-platform` returns an unfiltered `packages` array but
 * filtered `resolve.nodes[].deps`. The nodes are the authority: traverse them,
 * include normal and build edges, and skip edges that are only dev. The root
 * package is the application component, never a second library component.
 */
export function collectCargoClosure({ packages, resolve: resolution }) {
  if (!resolution || typeof resolution.root !== "string") {
    throw new Error("cargo metadata has no resolve.root");
  }
  const packageById = new Map(packages.map((entry) => [entry.id, entry]));
  const nodeById = new Map((resolution.nodes ?? []).map((entry) => [entry.id, entry]));
  if (!nodeById.has(resolution.root)) {
    throw new Error(`cargo metadata has no resolve node for root ${resolution.root}`);
  }

  const components = new Map();
  const seen = new Set([resolution.root]);
  const queue = [resolution.root];

  while (queue.length > 0) {
    const id = queue.shift();
    const node = nodeById.get(id);
    if (!node) throw new Error(`cargo metadata has no resolve node for ${id}`);

    for (const dep of node.deps ?? []) {
      const kinds = dep.dep_kinds ?? [];
      const include = kinds.length === 0 || kinds.some((entry) => entry.kind !== "dev");
      if (!include) continue;
      if (dep.pkg === resolution.root) continue;
      if (seen.has(dep.pkg)) continue;

      const pkg = packageById.get(dep.pkg);
      if (!pkg) throw new Error(`cargo metadata has no package for dependency ${dep.pkg}`);
      seen.add(dep.pkg);
      queue.push(dep.pkg);
      const purl = cargoPurl(pkg.name, pkg.version);
      if (!components.has(purl)) {
        components.set(
          purl,
          withLicense(
            { type: "library", name: pkg.name, version: pkg.version, purl },
            licenseChoices(pkg.license),
          ),
        );
      }
    }
  }

  return [...components.values()].sort((a, b) => a.purl.localeCompare(b.purl));
}
