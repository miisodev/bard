/*
  css-audit — four checks the TypeScript gate cannot make.

  CSS fails silently. A duplicated selector is not an error, it is a rule that
  quietly wins; an undefined custom property is not an error, it is a value
  that resolves to nothing; a class nobody uses and a class nobody styles are
  both not errors, and both make the next edit harder to reason about. All four
  happened here while rewriting the panel, and none of them showed up in a
  build, a typecheck, or a screenshot — the stale rules were close enough
  visually to pass a glance.

    bun scripts/css-audit.mjs          # report
    bun scripts/css-audit.mjs --strict # report and fail the build

  Unused-class and unstyled-class reporting are advisory by default: classes
  can be composed at runtime, and a stylesheet this repository does not read
  may dress one, so both lists want a human read rather than an automatic
  failure.
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
  // A selector list spans lines (".a,\n.b {"), so the capture must allow
  // newlines. Matching only the final line reduced ".a, .b" to a bare ".b",
  // which then collided with the ordinary base-rule-then-override pattern and
  // reported it as a duplicate.
  const re = /(?:^|\n)([.#a-z:\[][^{}]*?)\s*\{/g;
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
// Some custom properties are only ever set inline from code — a per-instance
// colour on a swatch, say — so a definition can legitimately live in a .tsx.
for (const m of code.matchAll(/"(--[a-z0-9-]+)"/g)) defined.add(m[1]);
const used = new Set([...allCss.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]));
for (const v of used) {
  if (!defined.has(v)) problems.push(`undefined custom property  var(${v})`);
}

/* ── 3. classes defined in CSS but never referenced from code ───────────── */
const classes = new Set([...allCss.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));
const unused = [...classes].filter((c) => !new RegExp(`\\b${c}\\b`).test(code)).sort();
if (unused.length) notes.push(`unused classes (${unused.length}): ${unused.join(", ")}`);

/* ── 4. classes referenced from .tsx with no rule anywhere ──────────────── */
/*
  The reverse of check 3, and the one that found the colour preview: seven
  classes in a component, no rule for any of them, and nothing in the build,
  the typecheck or a screenshot to say so. Only `className` strings are read,
  and only tokens that look like class names — a name is a run of lowercase
  letters, digits and hyphens, which leaves camelCase identifiers, `${…}`
  expressions and half-built names like "h-" out. Advisory on purpose: a class
  can legitimately be styled by a window this repository does not read, or
  composed at runtime, and the list wants a human eye rather than a red gate.
*/
function classNameCandidates(text) {
  const found = new Set();
  for (const m of text.matchAll(/className\s*=\s*(?:"([^"]*)"|'([^']*)'|\{`([^`]*)`\})/g)) {
    const source = m[1] ?? m[2] ?? m[3] ?? "";
    for (const token of source.replace(/\$\{[^}]*\}/g, " ").split(/\s+/)) {
      if (/^[a-z][a-z0-9-]*[a-z0-9]$/.test(token)) found.add(token);
    }
  }
  return found;
}

const styledSomewhere = (name) => new RegExp(`\\.${name}(?![\\w-])`).test(allCss);
const unstyled = new Map();
for (const file of files.filter((f) => extname(f) === ".tsx")) {
  for (const name of classNameCandidates(readFileSync(file, "utf8"))) {
    if (!styledSomewhere(name) && !unstyled.has(name)) unstyled.set(name, file);
  }
}
const unstyledNames = [...unstyled].sort((a, b) => a[0].localeCompare(b[0]));

/* ── report ─────────────────────────────────────────────────────────────── */
for (const n of notes) console.log(`note  ${n}`);
for (const [name, file] of unstyledNames) console.log(`note  no CSS rule  .${name}  (${file})`);
for (const p of problems) console.log(`FAIL  ${p}`);

if (problems.length) {
  console.log(`\ncss-audit: ${problems.length} problem(s)`);
  if (strict) process.exit(1);
} else {
  console.log(`css-audit: clean (${seen.size} rules, ${defined.size} tokens)`);
}
