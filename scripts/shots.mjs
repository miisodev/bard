// Store and site screenshots: every shot in src/dev/stage.html, rendered at 2×
// (3200 × 1800) by headless Edge against a running `bunx vite` dev server.
//
//   bunx vite --port 1430 &
//   node scripts/shots.mjs [out-dir] [base-url]
import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const out = resolve(process.argv[2] ?? "../.agents/files/Store submission/Screenshots");
const base = process.argv[3] ?? "http://localhost:1430";
const edge = "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
const shots = ["capture", "dictate", "draw", "models", "dark"];

mkdirSync(out, { recursive: true });
shots.forEach((shot, index) => {
  const file = resolve(out, `Screenshot ${index + 1} - ${shot}.png`);
  execFileSync(edge, [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    "--window-size=1600,900",
    "--force-device-scale-factor=2",
    "--virtual-time-budget=4000",
    `--screenshot=${file}`,
    `${base}/src/dev/stage.html?shot=${shot}`,
  ], { stdio: "ignore" });
  console.log(file);
});
