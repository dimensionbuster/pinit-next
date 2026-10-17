// Generates extension-dev/ from extension/: same code, different identity, so a developer
// can have the real PinIt extension and the dev extension installed side by side.
//   - own extension ID (key from scripts/dev-extension.json)
//   - own native host name (com.pinit.next.dev) -> debug builds register only that name
//   - menu entry says "PinIt DEV"
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const PROD_HOST = "com.pinit.next";
export const DEV_HOST = "com.pinit.next.dev";

// Appended to the dev copy only: reloads the extension when `npm run dev:app` rewrote its files,
// so the "remove folder, restart, reload in chrome://extensions" round trip is not needed.
// BUILD is baked into the running code and compared with dev-build.txt on disk, so a change
// made while the service worker slept is still noticed when it wakes.
const devReloadSnippet = (build) => `
// DEV only: reload when dev:app has written a newer copy of this extension.
(() => {
  const BUILD = "${build}";
  const check = () =>
    fetch(chrome.runtime.getURL("dev-build.txt"), { cache: "no-store" })
      .then((r) => r.text())
      .then((t) => {
        if (t.trim() !== BUILD) chrome.runtime.reload();
      })
      .catch(() => {});
  chrome.alarms.create("pinit-dev-reload", { periodInMinutes: 0.5 });
  chrome.alarms.onAlarm.addListener((a) => {
    if (a.name === "pinit-dev-reload") check();
  });
  check();
})();
`;

// Copies by writing into the existing files, never deleting them: Chrome keeps the loaded
// extension's files open, and on Windows `cpSync` (which unlinks the target first) then fails.
function copyInPlace(srcDir, outDir) {
  mkdirSync(outDir, { recursive: true });
  for (const entry of readdirSync(srcDir, { withFileTypes: true })) {
    if (/\.test\.mjs$/.test(entry.name)) continue;
    const from = join(srcDir, entry.name);
    const to = join(outDir, entry.name);
    if (entry.isDirectory()) {
      copyInPlace(from, to);
    } else {
      const bytes = readFileSync(from);
      let same = false;
      try {
        same = readFileSync(to).equals(bytes);
      } catch {}
      if (!same) writeFileSync(to, bytes);
    }
  }
}

export function buildDevExtension(srcDir, outDir, dev) {
  // overwritten in place (no rmSync): Chrome keeps pointing at a folder that never disappears
  copyInPlace(srcDir, outDir);

  const manifestPath = join(outDir, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  manifest.name = "PinIt (DEV)";
  manifest.key = dev.key;
  manifest.action = { default_title: "PinIt (DEV)" };
  manifest.permissions = [...manifest.permissions, "alarms"]; // for the auto reload below
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

  const bgPath = join(outDir, "background.js");
  let bg = readFileSync(bgPath, "utf8");
  for (const [from, to] of [
    [`const HOST = "${PROD_HOST}";`, `const HOST = "${DEV_HOST}";`],
    [`"PinIt으로 열기"`, `"PinIt DEV로 열기"`],
    [`"PinIt"`, `"PinIt (DEV)"`],
    [`"PinIt — "`, `"PinIt (DEV) — "`],
  ]) {
    if (!bg.includes(from)) throw new Error(`background.js no longer contains ${from}; update dev-extension.mjs`);
    bg = bg.split(from).join(to);
  }
  const build = String(Date.now());
  writeFileSync(bgPath, bg + devReloadSnippet(build));
  // written last: the running copy only reloads once everything else is in place
  writeFileSync(join(outDir, "dev-build.txt"), build);
}

// CLI: node scripts/dev-extension.mjs
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const dev = JSON.parse(readFileSync(join(root, "scripts", "dev-extension.json"), "utf8"));
  buildDevExtension(join(root, "extension"), join(root, "extension-dev"), dev);
  console.log("extension-dev/ ready (ID", dev.id + ")");
}
