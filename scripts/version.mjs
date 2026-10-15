// Keeps the app version in sync across package.json, src-tauri/Cargo.toml,
// src-tauri/tauri.conf.json and Cargo.lock.
//   node scripts/version.mjs         print current version (fails if files disagree)
//   node scripts/version.mjs --sync  copy package.json's version into the other files and `git add` them
// Bump with `npm version <x.y.z|patch|minor|major>`: it edits package.json, runs the "version" script
// (= --sync) so the other files ride along in npm's commit, then commits "<version>" and tags v<version>.
// Releases: bump on dev, merge to main (merge commit, not squash, so the tag stays reachable)
// -> .github/workflows/release.yml drafts v<version>.
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const file = (p) => root + p;
const read = (p) => readFileSync(file(p), "utf8");

const targets = [
  { path: "package.json", re: /("version"\s*:\s*")([^"]*)(")/ },
  { path: "src-tauri/tauri.conf.json", re: /("version"\s*:\s*")([^"]*)(")/ },
  { path: "src-tauri/Cargo.toml", re: /(^version\s*=\s*")([^"]*)(")/m },
  { path: "Cargo.lock", re: /(name = "pinit-next"\r?\nversion = ")([^"]*)(")/ },
];

const current = targets.map((t) => {
  const m = read(t.path).match(t.re);
  if (!m) throw new Error(`${t.path}: version not found`);
  return { ...t, version: m[2] };
});

if (process.argv.includes("--sync")) {
  const next = current[0].version; // package.json is the source of truth
  const changed = [];
  for (const t of current.slice(1)) {
    if (t.version === next) continue;
    writeFileSync(file(t.path), read(t.path).replace(t.re, `$1${next}$3`));
    console.log(`${t.path}: ${t.version} -> ${next}`);
    changed.push(t.path);
  }
  if (changed.length) {
    const r = spawnSync("git", ["add", "--", ...changed], { cwd: root, encoding: "utf8" });
    if (r.status !== 0) {
      console.error(`git add failed:\n${r.stderr || r.stdout}`);
      process.exit(1);
    }
  }
} else {
  const versions = new Set(current.map((c) => c.version));
  if (versions.size !== 1) {
    console.error("version mismatch:\n" + current.map((c) => `  ${c.path}: ${c.version}`).join("\n"));
    process.exit(1);
  }
  console.log([...versions][0]);
}
