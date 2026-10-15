// Single place to read/bump the app version across package.json, src-tauri/Cargo.toml,
// src-tauri/tauri.conf.json and Cargo.lock.
//   node scripts/version.mjs            print current version (fails if files disagree)
//   node scripts/version.mjs 0.2.0      set all files to 0.2.0, commit them as "0.2.0" and tag v0.2.0
//   node scripts/version.mjs 0.2.0 --no-git   only edit the files
// Like `npm version`: the tag lets you `git diff v0.1.0..v0.2.0` locally. Releases: bump on dev, merge to
// main (merge commit, not squash, so the tag stays reachable) -> .github/workflows/release.yml drafts v<version>.
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

const args = process.argv.slice(2);
const next = args.find((a) => !a.startsWith("--"));
const git = (...a) => {
  const r = spawnSync("git", a, { cwd: root, encoding: "utf8" });
  if (r.status !== 0) {
    console.error(`git ${a.join(" ")} failed:\n${r.stderr || r.stdout}`);
    process.exit(1);
  }
  return r.stdout.trim();
};

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
  const tag = `v${next}`;
  const useGit = !args.includes("--no-git");
  if (useGit && git("tag", "--list", tag)) {
    console.error(`tag ${tag} already exists`);
    process.exit(1);
  }
  for (const t of current) {
    writeFileSync(file(t.path), read(t.path).replace(t.re, `$1${next}$3`));
    console.log(`${t.path}: ${t.version} -> ${next}`);
  }
  if (useGit) {
    const paths = current.map((t) => t.path);
    git("add", "--", ...paths);
    git("commit", "-m", next, "--", ...paths); // other staged/unstaged work is left out of the version commit
    git("tag", "-a", tag, "-m", tag);
    console.log(`committed "${next}" and tagged ${tag}`);
  }
}
