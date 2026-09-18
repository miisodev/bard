import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const SIZE = 1024;
const svg = `<svg width="${SIZE}" height="${SIZE}" viewBox="0 0 ${SIZE} ${SIZE}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#63BCF2"/>
      <stop offset="1" stop-color="#1E6FA8"/>
    </linearGradient>
  </defs>
  <rect x="64" y="64" width="896" height="896" rx="216" fill="url(#bg)"/>
  <rect x="64" y="64" width="896" height="896" rx="216" fill="none" stroke="rgba(255,255,255,0.18)" stroke-width="8"/>
  <rect x="336" y="396" width="96" height="232" rx="48" fill="white" opacity="0.85"/>
  <rect x="464" y="286" width="96" height="452" rx="48" fill="white"/>
  <rect x="592" y="352" width="96" height="320" rx="48" fill="white" opacity="0.9"/>
</svg>`;

const png = await sharp(Buffer.from(svg)).png().toBuffer();
const out = join(dirname(fileURLToPath(import.meta.url)), "..", "app-icon.png");
writeFileSync(out, png);
console.log("app-icon.png written:", png.length, "bytes");