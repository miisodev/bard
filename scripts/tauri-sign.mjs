/*
  tauri-sign.mjs — sign one release artefact with the Tauri updater key.

  This exists because of where the key is allowed to be.

  `bun run tauri signer sign -p … <file>` is the documented flow, and it is
  correct as far as the signature goes. But it puts the updater private key in
  the environment of `bun`, of the JavaScript in `node_modules/@tauri-apps/cli`
  that bun then loads, and of every lifecycle script bun decides to run. A
  release key that is in a build's environment is a key that ~400 transitive
  npm packages and every `build.rs` in the Cargo graph are one bad `postinstall`
  away from reading — and because the signature they would produce is
  *genuinely valid*, `tauri-plugin-updater` would accept their installer on
  every installed client and nobody would ever know. It is the one secret in
  this project whose compromise is both silent and total.

  So: build with the signing environment unset, and sign in its own step. This
  file is that step's smallest possible interpreter. It dlopens the platform's
  *native* Tauri CLI binding directly, so no JavaScript from `node_modules` is
  evaluated in the process that can read the key, and the key never appears in
  a command line. The password reaches the CLI through
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, which is set here from a file or from a
  named environment variable and scrubbed immediately afterwards.

  Verify whatever it signs with `verify-release.ps1`, which is what
  `examples/verify_updater.rs` runs.

  Usage (from the repository root):

    node scripts/tauri-sign.mjs --target <file> --key <keyfile> \
      [--password-file <file> | --password-env <NAME>]

  --password-file / --password-env are how the key's password is supplied.
  With neither, an already-exported TAURI_SIGNING_PRIVATE_KEY_PASSWORD is used.
  The password is never accepted as an argument, because arguments are visible
  to every process on the machine.
*/

import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PASSWORD_ENV = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD";

function fail(message) {
  console.error(`tauri-sign: ${message}`);
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

/**
 * The platform's native CLI binding, loaded without going through
 * `node_modules/@tauri-apps/cli/index.js` (or `tauri.js`, or `main.js`).
 *
 * Those three files are ordinary JavaScript from the dependency graph, and
 * loading them is exactly the thing this script exists to avoid. The `.node`
 * file is a NAPI addon: `createRequire` dlopens it and the only exports are the
 * CLI's own `run` and `logError`.
 */
function loadNativeCli() {
  const require = createRequire(import.meta.url);
  const scope = join(repoRoot, "node_modules", "@tauri-apps");
  if (!existsSync(scope)) fail("node_modules/@tauri-apps is missing. Run `bun install`.");

  const prefix = `cli.${process.platform}-${process.arch}`;
  const found = [];
  for (const entry of readdirSync(scope)) {
    if (!entry.startsWith("cli-")) continue;
    const dir = join(scope, entry);
    let names;
    try {
      if (!statSync(dir).isDirectory()) continue;
      names = readdirSync(dir);
    } catch {
      continue;
    }
    for (const name of names) {
      if (name.startsWith(prefix) && name.endsWith(".node")) found.push(join(dir, name));
    }
  }
  if (found.length === 0) {
    fail(`no native Tauri CLI binding for ${process.platform}-${process.arch} under ${scope}`);
  }
  // Prefer the MSVC build on Windows: it is the one the Rust toolchain here
  // targets, and its file name is the most specific match.
  found.sort((a, b) => Number(b.endsWith("-msvc.node")) - Number(a.endsWith("-msvc.node")));
  return require(found[0]);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

const options = parseArgs(process.argv.slice(2));
const target = options.target ? resolve(options.target) : null;
const keyPath = options.key ? resolve(options.key) : null;
if (!target) fail("usage: tauri-sign.mjs --target <file> --key <keyfile> [--password-file <file> | --password-env <NAME>]");
if (!keyPath) fail("--key is required (the path to the minisign private key file)");
if (!existsSync(target)) fail(`no such artefact: ${target}`);
if (!existsSync(keyPath)) fail(`no such key file: ${keyPath}`);

let password = process.env[PASSWORD_ENV] ?? "";
if (options["password-file"]) {
  password = readFileSync(resolve(options["password-file"]), "utf8").trim();
} else if (options["password-env"]) {
  password = process.env[options["password-env"]] ?? "";
  if (!password) fail(`$${options["password-env"]} is empty`);
}

// TAURI_SIGNING_PRIVATE_KEY (the key material itself) is the one variable this
// step must not have, because the CLI would prefer it over --key-path and the
// material would end up in this process's environment. Fail loudly instead.
if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
  fail("TAURI_SIGNING_PRIVATE_KEY is set in this process. Unset it: the key is read from a file, never from the environment.");
}

const cli = loadNativeCli();
process.env[PASSWORD_ENV] = password;
let code = 0;
try {
  await new Promise((done, failPromise) => {
    // The binding is argv *without* the program name, exactly as tauri.js
    // hands it over after stripping argv[0] and argv[1].
    cli.run(["signer", "sign", "-f", keyPath, target], null, (error) => {
      if (error) failPromise(error instanceof Error ? error : new Error(String(error)));
      else done();
    });
  });
} catch (error) {
  cli.logError?.(error?.message ?? String(error));
  code = 1;
} finally {
  // Scrub before anything else can observe the process.
  delete process.env[PASSWORD_ENV];
  password = "";
}

if (code !== 0) process.exit(code);

const signature = `${target}.sig`;
if (!existsSync(signature)) fail(`the CLI reported success but ${signature} does not exist`);
console.log(`signed    ${target}`);
console.log(`  bytes   ${statSync(target).size}`);
console.log(`  sha256  ${sha256(target)}`);
console.log(`  sig     ${signature} (${statSync(signature).size} bytes)`);
