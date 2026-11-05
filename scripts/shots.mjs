// Store and site screenshots: every shot in src/dev/stage.html, rendered by
// headless Edge against a running `bunx vite` dev server.
//
//   bunx vite --port 1430 &
//   node scripts/shots.mjs                       # Store set, defaults below
//   node scripts/shots.mjs --gallery             # launch gallery preset
//   node scripts/shots.mjs <out-dir> <base-url>  # positionals, as before
//
// The defaults are the Store submission's own values, so a bare run still
// writes exactly what it always did. The gallery preset is separate and never
// writes into the Store folder unless it is pointed there explicitly.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { resolve } from "node:path";

const STORE_OUT = "../.agents/files/Store submission/Screenshots";
const STORE_NAME = "Screenshot {n} - {shot}.png";
const STORE_ORDER = ["capture", "dictate", "draw", "models", "dark"];
const GALLERY_OUT = "../.agents/files/launch/gallery";
const GALLERY_NAME = "{n}-{shot}.png";
const GALLERY_ORDER = ["dictate", "capture", "models", "draw", "dark"];
const DEFAULT_SIZE = "1600x900";
const DEFAULT_SCALE = "2";
const DEFAULT_BASE = "http://localhost:1430";
const BROWSERS = process.platform === "win32"
  ? ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Google/Chrome/Application/chrome.exe"]
  : ["/usr/bin/google-chrome", "/opt/google/chrome/chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"];

const HELP = `Render the screenshots in src/dev/stage.html with headless Chrome or Edge.

Usage
  node scripts/shots.mjs [out-dir] [base-url] [options]

  Run it with the dev server already up:  bunx vite --port 1430

Options
  --out <dir>        output directory (default "${STORE_OUT}")
  --size <WxH>       layout size in CSS pixels (default ${DEFAULT_SIZE})
  --scale <n>        device scale factor (default ${DEFAULT_SCALE})
  --name <pattern>   file name; {n} is the 1-based position, {shot} the shot
                     name (default "${STORE_NAME}")
  --order <a,b,...>  shot names in render order
                     (default ${STORE_ORDER.join(",")})
  --browser <path>   Chrome/Edge executable (or BARD_SCREENSHOT_BROWSER)
  --base <url>       dev-server base URL (default ${DEFAULT_BASE})
  --gallery          launch-gallery preset: 1600x900 at 1.2x, names
                     "${GALLERY_NAME}", order ${GALLERY_ORDER.join(",")},
                     writing to "${GALLERY_OUT}"
  --dry-run          print what would render; launch nothing, write nothing
  -h, --help         print this

The Store set is ${STORE_OUT} at ${DEFAULT_SIZE} exported at ${DEFAULT_SCALE}x
(3200 x 1800), the format the Store listing carries. The gallery preset is
1600 x 900 exported at 1.2x (1920 x 1080), which
clears the kit's 1920 x 1080 floor — see .agents/files/launch/LAUNCH.md.

Relative paths, the defaults included, resolve against the working directory:
run it from the repository root, as the commands above do.`;

function fail(message) {
  console.error(`shots: ${message}`);
  throw new Error(`shots: ${message}`);
}

function parseSize(value) {
  const match = /^(\d+)x(\d+)$/.exec(value);
  if (!match) fail(`--size wants WxH, not "${value}".`);
  const width = Number(match[1]);
  const height = Number(match[2]);
  if (!width || !height) fail(`--size wants positive numbers, not "${value}".`);
  return `${width},${height}`;
}

const argv = process.argv.slice(2);
const options = {};
const positional = [];
for (let i = 0; i < argv.length; i += 1) {
  const arg = argv[i];
  if (arg === "--help" || arg === "-h") {
    console.log(HELP);
    process.exit(0);
  }
  if (arg === "--gallery") {
    options.gallery = true;
    continue;
  }
  if (arg === "--dry-run") {
    options.dryRun = true;
    continue;
  }
  if (arg.startsWith("--")) {
    const name = arg.slice(2);
    const value = argv[i + 1];
    if (value === undefined || value.startsWith("--")) fail(`--${name} needs a value.`);
    options[name] = value;
    i += 1;
    continue;
  }
  positional.push(arg);
}

for (const name of Object.keys(options)) {
  if (!["gallery", "dryRun", "out", "size", "scale", "name", "order", "base", "browser"].includes(name)) {
    fail(`unknown option --${name}. Run with --help.`);
  }
}

const preset = options.gallery
  ? { out: GALLERY_OUT, name: GALLERY_NAME, order: GALLERY_ORDER, size: "1600x900", scale: "1.2" }
  : { out: STORE_OUT, name: STORE_NAME, order: STORE_ORDER, size: DEFAULT_SIZE };

const out = resolve(options.out ?? positional[0] ?? preset.out);
const base = options.base ?? positional[1] ?? DEFAULT_BASE;
const windowSize = parseSize(options.size ?? preset.size);
const scale = Number(options.scale ?? preset.scale ?? DEFAULT_SCALE);
if (!Number.isFinite(scale) || scale <= 0) fail(`--scale wants a positive number, not "${options.scale}".`);
const pattern = options.name ?? preset.name;
if (!pattern.includes("{shot}")) fail(`--name wants a {shot} placeholder, not "${pattern}".`);
const shots = (options.order ?? preset.order.join(","))
  .split(",")
  .map((shot) => shot.trim())
  .filter(Boolean);
if (shots.length === 0) fail("--order needs at least one shot name.");

const [width, height] = windowSize.split(",").map(Number);
const browser = options.browser ?? process.env.BARD_SCREENSHOT_BROWSER ?? BROWSERS.find(existsSync);
if (!options.dryRun && (!browser || !existsSync(browser))) {
  fail("Chrome/Edge was not found. Supply --browser <path> or BARD_SCREENSHOT_BROWSER.");
}
if (!options.dryRun) {
  try {
    const response = await fetch(`${base}/src/dev/stage.html`, { signal: AbortSignal.timeout(3000) });
    if (!response.ok) fail(`preview server returned HTTP ${response.status}.`);
  } catch { fail(`preview server is not reachable at ${base}. Start it before rendering.`); }
}
if (!options.dryRun) mkdirSync(out, { recursive: true });
if (options.dryRun) {
  console.log(
    `shots: dry run — out=${out} size=${windowSize} scale=${scale} order=${shots.join(",")} name="${pattern}"`,
  );
}
shots.forEach((shot, index) => {
  const file = resolve(out, pattern.replaceAll("{n}", String(index + 1)).replaceAll("{shot}", shot));
  const args = [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    `--window-size=${windowSize}`,
    `--force-device-scale-factor=${scale}`,
    "--virtual-time-budget=4000",
    `--screenshot=${file}`,
    `${base}/src/dev/stage.html?shot=${shot}`,
  ];
  if (options.dryRun) {
    console.log(`"${browser ?? "<Chrome/Edge>"}" ${args.map((arg) => (arg.includes(" ") ? `"${arg}"` : arg)).join(" ")}`);
    return;
  }
  // An isolated, disposable profile avoids attaching to the person's browser.
  const profile = mkdtempSync(join(out, ".bard-render-"));
  const staged = join(profile, "capture.png");
  try {
    const renderArgs = args.map((arg) => arg.startsWith("--screenshot=") ? `--screenshot=${staged}` : arg);
    execFileSync(browser, [...renderArgs, `--user-data-dir=${profile}`], { stdio: "ignore", timeout: 20000 });
    if (!existsSync(staged)) fail(`the browser produced no screenshot for ${shot}.`);
    const png = readFileSync(staged);
    if (png.length < 24 || png.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") fail(`${shot} is not a PNG.`);
    const actualWidth = png.readUInt32BE(16);
    const actualHeight = png.readUInt32BE(20);
    if (actualWidth !== Math.round(width * scale) || actualHeight !== Math.round(height * scale)) {
      fail(`${shot} is ${actualWidth}x${actualHeight}, expected ${Math.round(width * scale)}x${Math.round(height * scale)}.`);
    }
    renameSync(staged, file);
    console.log(file);
  } finally { rmSync(profile, { recursive: true, force: true }); }
});
if (options.dryRun) {
  console.log(
    `shots: ${shots.length} frames at ${width * scale} x ${height * scale} px; nothing rendered, nothing written.`,
  );
}
