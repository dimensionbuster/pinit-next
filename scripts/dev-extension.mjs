// Generates extension-dev/ from extension/: same code, different identity, so a developer
// can have the real PinIt extension and the dev extension installed side by side.
//   - own extension ID (key from scripts/dev-extension.json)
//   - own native host name (com.pinit.next.dev) -> debug builds register only that name
//   - menu entry says "PinIt DEV"
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const PROD_HOST = "com.pinit.next";
export const DEV_HOST = "com.pinit.next.dev";

export function buildDevExtension(srcDir, outDir, dev) {
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  cpSync(srcDir, outDir, { recursive: true, filter: (p) => !/\.test\.mjs$/.test(p) });

  const manifestPath = join(outDir, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  manifest.name = "PinIt (DEV)";
  manifest.key = dev.key;
  manifest.action = { default_title: "PinIt (DEV)" };
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
  writeFileSync(bgPath, bg);
}

// CLI: node scripts/dev-extension.mjs
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const dev = JSON.parse(readFileSync(join(root, "scripts", "dev-extension.json"), "utf8"));
  buildDevExtension(join(root, "extension"), join(root, "extension-dev"), dev);
  console.log("extension-dev/ ready (ID", dev.id + ")");
}
