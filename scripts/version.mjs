// Single place to read/bump the app version across package.json, src-tauri/Cargo.toml,
// src-tauri/tauri.conf.json and Cargo.lock.
//   node scripts/version.mjs            print current version (fails if files disagree)
//   node scripts/version.mjs 0.2.0      set all files to 0.2.0
// Releases: bump, commit, merge to main -> .github/workflows/release.yml drafts v<version>.
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

const next = process.argv[2];
if (!next) {
  const versions = new Set(current.map((c) => c.version));
  if (versions.size !== 1) {
    console.error("version mismatch:\n" + current.map((c) => `  ${c.path}: ${c.version}`).join("\n"));
    process.exit(1);
  }
  console.log([...versions][0]);
} else {
  if (!/^\d+\.\d+\.\d+$/.test(next)) {
    console.error(`invalid version "${next}" (expected x.y.z)`);
    process.exit(1);
  }
  for (const t of current) {
    writeFileSync(file(t.path), read(t.path).replace(t.re, `$1${next}$3`));
    console.log(`${t.path}: ${t.version} -> ${next}`);
  }
}
