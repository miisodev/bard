import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  collectCargoClosure,
  collectNpmClosure,
  createFsPackageReader,
  isFullCommitSha,
  npmPurl,
  parseCargoLockPackage,
  parseCargoPackageIdentity,
  validateSourceIdentity,
} from "./release-metadata-lib.mjs";

function fixtureReader(entries) {
  return (name, fromDir) => entries.get(`${fromDir}\u0000${name}`) ?? null;
}

function installed(dir, name, version, extra = {}) {
  return { dir, packageJson: { name, version, ...extra } };
}

test("npm closure follows hoisted/scoped transitive deps and skips optional/peer absences", () => {
  const root = "/repo";
  const entries = new Map();
  const add = (fromDir, name, found) => entries.set(`${fromDir}\u0000${name}`, found);

  add(
    root,
    "a",
    installed("/repo/node_modules/a", "a", "1.0.0", {
      dependencies: { b: "1", d: "1" },
      optionalDependencies: { "@platform/missing": "1" },
      devDependencies: { "dev-only": "1" },
    }),
  );
  add("/repo/node_modules/a", "b", installed("/repo/node_modules/b", "b", "2.0.0", {
    dependencies: { "@scope/c": "1" },
  }));
  add(
    "/repo/node_modules/a",
    "d",
    installed("/repo/node_modules/a/node_modules/d", "d", "3.0.0"),
  );
  add(
    "/repo/node_modules/b",
    "@scope/c",
    installed("/repo/node_modules/@scope/c", "@scope/c", "4.0.0"),
  );
  add(
    root,
    "@scope/root-opt",
    installed("/repo/node_modules/@scope/root-opt", "@scope/root-opt", "5.0.0"),
  );

  const components = collectNpmClosure({
    rootDir: root,
    dependencies: ["a"],
    optionalDependencies: ["@scope/root-opt", "@platform/root-missing"],
    peerDependencies: ["peer-missing"],
    peerDependenciesMeta: { "peer-missing": { optional: true } },
    readPackage: fixtureReader(entries),
  });

  assert.deepEqual(
    components.map((component) => component.purl),
    [
      "pkg:npm/%40scope/c@4.0.0",
      "pkg:npm/%40scope/root-opt@5.0.0",
      "pkg:npm/a@1.0.0",
      "pkg:npm/b@2.0.0",
      "pkg:npm/d@3.0.0",
    ],
  );
  assert.equal(npmPurl("@scope/pkg", "1.0.0"), "pkg:npm/%40scope/pkg@1.0.0");
});

test("npm closure fails a missing required transitive dependency but skips optional ones", () => {
  const root = "/repo";
  const entries = new Map();
  entries.set(
    `${root}\u0000a`,
    installed("/repo/node_modules/a", "a", "1.0.0", {
      dependencies: { missing: "1" },
      optionalDependencies: { "@platform/missing": "1" },
    }),
  );

  assert.throws(
    () =>
      collectNpmClosure({
        rootDir: root,
        dependencies: ["a"],
        readPackage: fixtureReader(entries),
      }),
    /missing required npm dependency "missing" required by root > a/,
  );
});

test("npm closure fails a missing required peer unless it is marked optional", () => {
  const root = "/repo";
  const entries = new Map([
    [
      `${root}\u0000a`,
      installed("/repo/node_modules/a", "a", "1.0.0", {
        peerDependencies: { peer: "1" },
      }),
    ],
  ]);
  assert.throws(
    () =>
      collectNpmClosure({
        rootDir: root,
        dependencies: ["a"],
        readPackage: fixtureReader(entries),
      }),
    /missing required npm dependency "peer" required by root > a/,
  );

  const optional = new Map([
    [
      `${root}\u0000a`,
      installed("/repo/node_modules/a", "a", "1.0.0", {
        peerDependencies: { peer: "1" },
        peerDependenciesMeta: { peer: { optional: true } },
      }),
    ],
  ]);
  assert.deepEqual(
    collectNpmClosure({
      rootDir: root,
      dependencies: ["a"],
      readPackage: fixtureReader(optional),
    }).map((component) => component.name),
    ["a"],
  );
});

test("npm closure describes an installed alias by its real package name", () => {
  const root = "/repo";
  const entries = new Map([
    [`${root}\u0000alias`, installed("/repo/node_modules/alias", "real-name", "1.2.3")],
  ]);
  const components = collectNpmClosure({
    rootDir: root,
    dependencies: ["alias"],
    readPackage: fixtureReader(entries),
  });
  assert.deepEqual(
    components.map((component) => ({ name: component.name, purl: component.purl })),
    [{ name: "real-name", purl: "pkg:npm/real-name@1.2.3" }],
  );
});

test("npm closure rejects a path-like package name instead of reading outside node_modules", () => {
  assert.throws(
    () =>
      collectNpmClosure({
        rootDir: "/repo",
        dependencies: ["../../secret"],
        readPackage: () => null,
      }),
    /invalid npm dependency name "\.\.\/\.\.\/secret"/,
  );
});

test("filesystem reader resolves hoisted, nested and scoped packages", () => {
  const root = mkdtempSync(join(tmpdir(), "bard-release-meta-"));
  const write = (relative, json) => {
    const dir = join(root, relative);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "package.json"), JSON.stringify(json));
  };
  try {
    write("node_modules/a", { name: "a", version: "1.0.0" });
    write("node_modules/b", { name: "b", version: "1.0.0" });
    write("node_modules/@scope/c", { name: "@scope/c", version: "1.0.0" });
    write("node_modules/a/node_modules/d", { name: "d", version: "1.0.0" });

    const read = createFsPackageReader(root);
    assert.equal(read("b", join(root, "node_modules/a"))?.dir, join(root, "node_modules/b"));
    assert.equal(
      read("d", join(root, "node_modules/a"))?.dir,
      join(root, "node_modules/a/node_modules/d"),
    );
    assert.equal(
      read("@scope/c", join(root, "node_modules/b"))?.dir,
      join(root, "node_modules/@scope/c"),
    );
    assert.equal(read("missing", join(root, "node_modules/a")), null);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("cargo closure includes normal/build, excludes dev-only and target-filtered packages, and omits root", () => {
  const packages = [
    { id: "root", name: "bard", version: "2.2.0", license: "MIT" },
    { id: "serde", name: "serde", version: "1.0.0", license: "MIT OR Apache-2.0" },
    { id: "serde_derive", name: "serde_derive", version: "1.0.0", license: "MIT OR Apache-2.0" },
    { id: "tauri-build", name: "tauri-build", version: "2.0.0", license: "MIT OR Apache-2.0" },
    { id: "cc", name: "cc", version: "1.0.0", license: "MIT OR Apache-2.0" },
    { id: "criterion", name: "criterion", version: "0.5.0", license: "Apache-2.0" },
    { id: "criterion-dep", name: "criterion-dep", version: "1.0.0", license: "MIT" },
    { id: "windows-only", name: "windows-only", version: "1.0.0", license: "MIT" },
    { id: "linux-only", name: "linux-only", version: "1.0.0", license: "MIT" },
  ];
  const resolve = {
    root: "root",
    nodes: [
      {
        id: "root",
        deps: [
          { pkg: "serde", dep_kinds: [{ kind: null, target: null }] },
          { pkg: "tauri-build", dep_kinds: [{ kind: "build", target: null }] },
          { pkg: "criterion", dep_kinds: [{ kind: "dev", target: null }] },
          { pkg: "linux-only", dep_kinds: [{ kind: null, target: "cfg(unix)" }] },
        ],
      },
      {
        id: "serde",
        deps: [{ pkg: "serde_derive", dep_kinds: [{ kind: null, target: null }] }],
      },
      {
        id: "tauri-build",
        deps: [{ pkg: "cc", dep_kinds: [{ kind: "build", target: null }] }],
      },
      {
        id: "criterion",
        deps: [{ pkg: "criterion-dep", dep_kinds: [{ kind: null, target: null }] }],
      },
      {
        id: "linux-only",
        deps: [],
      },
      {
        id: "serde_derive",
        deps: [],
      },
      {
        id: "cc",
        deps: [],
      },
    ],
  };

  const components = collectCargoClosure({ packages, resolve });
  assert.deepEqual(
    components.map((component) => component.purl),
    [
      "pkg:cargo/cc@1.0.0",
      "pkg:cargo/linux-only@1.0.0",
      "pkg:cargo/serde_derive@1.0.0",
      "pkg:cargo/serde@1.0.0",
      "pkg:cargo/tauri-build@2.0.0",
    ],
  );
  assert.ok(!components.some((component) => component.purl.includes("bard")));
  assert.ok(!components.some((component) => component.purl.includes("criterion")));
  assert.ok(!components.some((component) => component.purl.includes("windows-only")));
});

test("cargo closure fails a resolved dependency missing from packages", () => {
  assert.throws(
    () =>
      collectCargoClosure({
        packages: [{ id: "root", name: "bard", version: "2.2.0" }],
        resolve: {
          root: "root",
          nodes: [
            {
              id: "root",
              deps: [{ pkg: "missing", dep_kinds: [{ kind: null, target: null }] }],
            },
          ],
        },
      }),
    /cargo metadata has no package for dependency missing/,
  );
});

test("source identity accepts matching product/version data and rejects mismatches", () => {
  const current = {
    package: { name: "bard", version: "2.2.0" },
    tauri: { productName: "Bard", version: "2.2.0" },
    cargo: { name: "bard", version: "2.2.0" },
    cargoLock: { name: "bard", version: "2.2.0" },
  };
  const source = structuredClone(current);

  assert.deepEqual(validateSourceIdentity({ source, current, version: "2.2.0" }), []);

  const wrongName = structuredClone(source);
  wrongName.package.name = "other";
  assert.match(
    validateSourceIdentity({ source: wrongName, current, version: "2.2.0" }).join("\n"),
    /package name differs: source other, current bard/,
  );

  const wrongPackage = structuredClone(source);
  wrongPackage.package.version = "2.1.1";
  assert.match(
    validateSourceIdentity({ source: wrongPackage, current, version: "2.2.0" }).join("\n"),
    /package version differs/,
  );

  const wrongCargo = structuredClone(source);
  wrongCargo.cargo.version = "2.1.0";
  assert.match(
    validateSourceIdentity({ source: wrongCargo, current, version: "2.2.0" }).join("\n"),
    /Cargo package version differs/,
  );
});

test("full commit SHA and Cargo identity parsers are strict", () => {
  assert.equal(isFullCommitSha("a".repeat(40)), true);
  assert.equal(isFullCommitSha("a".repeat(39)), false);
  assert.equal(isFullCommitSha("g".repeat(40)), false);

  const cargoToml = '[package]\nname = "bard"\nversion = "2.2.0"\n\n[dependencies]\nserde = "1"\n';
  assert.deepEqual(parseCargoPackageIdentity(cargoToml), { name: "bard", version: "2.2.0" });

  const cargoLock =
    '[[package]]\nname = "serde"\nversion = "1.0.0"\n\n[[package]]\nname = "bard"\nversion = "2.2.0"\n';
  assert.deepEqual(parseCargoLockPackage(cargoLock, "bard"), {
    name: "bard",
    version: "2.2.0",
  });
});

test("npm closure visits separate installs of the same version with different peer resolutions", () => {
  const entries = new Map([
    ["/repo\u0000a", installed("/repo/node_modules/a", "a", "1", { dependencies: { shared: "1" } })],
    ["/repo\u0000b", installed("/repo/node_modules/b", "b", "1", { dependencies: { shared: "1" } })],
    ["/repo/node_modules/a\u0000shared", installed("/repo/node_modules/a/node_modules/shared", "shared", "1", { peerDependencies: { peer: "*" } })],
    ["/repo/node_modules/b\u0000shared", installed("/repo/node_modules/b/node_modules/shared", "shared", "1", { peerDependencies: { peer: "*" } })],
    ["/repo/node_modules/a/node_modules/shared\u0000peer", installed("/repo/node_modules/a/node_modules/peer", "peer", "17")],
    ["/repo/node_modules/b/node_modules/shared\u0000peer", installed("/repo/node_modules/b/node_modules/peer", "peer", "18")],
  ]);
  const result = collectNpmClosure({ rootDir: "/repo", dependencies: ["a", "b"], readPackage: fixtureReader(entries) });
  assert(result.some((entry) => entry.purl === "pkg:npm/peer@18"));
  assert.equal(result.filter((entry) => entry.name === "shared").length, 1);
});

test("optional declarations override required declarations of the same package", () => {
  const entries = new Map([
    ["/repo\u0000a", installed("/repo/node_modules/a", "a", "1", { dependencies: { platform: "1" }, optionalDependencies: { platform: "1" } })],
  ]);
  assert.doesNotThrow(() => collectNpmClosure({ rootDir: "/repo", dependencies: ["a", "root-platform"], optionalDependencies: ["root-platform"], readPackage: fixtureReader(entries) }));
});
