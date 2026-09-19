/*
  css-audit — three checks the TypeScript gate cannot make.

  CSS fails silently. A duplicated selector is not an error, it is a rule that
  quietly wins; an undefined custom property is not an error, it is a value
  that resolves to nothing; a class nobody uses is not an error, it is dead
  weight that makes the next edit harder to reason about. All three happened
  here while rewriting the panel, and none of them showed up in a build, a
  typecheck, or a screenshot — the stale rules were close enough visually to
  pass a glance.

    bun scripts/css-audit.mjs          # report
    bun scripts/css-audit.mjs --strict # report and fail the build

  Unused-class reporting is advisory by default: classes can be composed at
  runtime, so the list wants a human read rather than an automatic failure.
*/
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname } from "node:path";

const SRC = "src";
const strict = process.argv.includes("--strict");

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

const files = walk(SRC);
const cssFiles = files.filter((f) => extname(f) === ".css");
const codeFiles = files.filter((f) => [".ts", ".tsx"].includes(extname(f)));

const css = cssFiles.map((f) => ({ file: f, text: readFileSync(f, "utf8") }));
const code = codeFiles.map((f) => readFileSync(f, "utf8")).join("\n");
const allCss = css.map((c) => c.text).join("\n");

const problems = [];
const notes = [];

/* ── 1. duplicate selectors ─────────────────────────────────────────────── */
// Top-level rules only: a selector at column 0, up to its opening brace.
const seen = new Map();
for (const { file, text } of css) {
  const re = /(?:^|\n)((?:\.[^{}\n]|[a-z:\[][^{}\n])[^{}\n]*?)\s*\{/g;
  let m;
  while ((m = re.exec(text))) {
    const sel = m[1].trim();
    if (sel.startsWith("@") || sel.startsWith("from") || sel.startsWith("to")) continue;
    // :root is layered deliberately — a base block, one per theme, and the
    // legacy-alias block — so repetition there is the design, not a mistake.
    if (sel === ":root" || sel.startsWith(":root")) continue;
    // Skip rules nested inside @media/@keyframes by checking brace depth.
    const before = text.slice(0, m.index);
    const depth = (before.match(/\{/g) || []).length - (before.match(/\}/g) || []).length;
    if (depth !== 0) continue;
    const key = sel.replace(/\s+/g, " ");
    if (seen.has(key)) problems.push(`duplicate selector  ${key}  (${seen.get(key)} and ${file})`);
    else seen.set(key, file);
  }
}

/* ── 2. custom properties used but never defined ────────────────────────── */
const defined = new Set([...allCss.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
const used = new Set([...allCss.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]));
for (const v of used) {
  if (!defined.has(v)) problems.push(`undefined custom property  var(${v})`);
}

/* ── 3. classes defined in CSS but never referenced from code ───────────── */
const classes = new Set([...allCss.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));
const unused = [...classes].filter((c) => !new RegExp(`\\b${c}\\b`).test(code)).sort();
if (unused.length) notes.push(`unused classes (${unused.length}): ${unused.join(", ")}`);

/* ── report ─────────────────────────────────────────────────────────────── */
for (const n of notes) console.log(`note  ${n}`);
for (const p of problems) console.log(`FAIL  ${p}`);

if (problems.length) {
  console.log(`\ncss-audit: ${problems.length} problem(s)`);
  if (strict) process.exit(1);
} else {
  console.log(`css-audit: clean (${seen.size} rules, ${defined.size} tokens)`);
}
