// Guards the invariants the native host relies on: fixed extension IDs and a faithful dev copy.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildDevExtension, DEV_HOST, PROD_HOST } from "../scripts/dev-extension.mjs";

const manifest = JSON.parse(readFileSync(new URL("./manifest.json", import.meta.url), "utf8"));
const dev = JSON.parse(readFileSync(new URL("../scripts/dev-extension.json", import.meta.url), "utf8"));
const rust = readFileSync(new URL("../src-tauri/src/native_host.rs", import.meta.url), "utf8");
const constant = (name) => new RegExp(`${name}: &str = "([^"]+)"`).exec(rust)?.[1];

function idFromKey(b64) {
  const hash = createHash("sha256").update(Buffer.from(b64, "base64")).digest().subarray(0, 16);
  return [...hash].map((b) => String.fromCharCode(97 + (b >> 4)) + String.fromCharCode(97 + (b & 15))).join("");
}

test("manifest keys derive the extension IDs hard-coded in native_host.rs", () => {
  assert.equal(idFromKey(manifest.key), constant("PROD_EXTENSION_ID"));
  assert.equal(idFromKey(dev.key), constant("DEV_EXTENSION_ID"));
  assert.equal(dev.id, constant("DEV_EXTENSION_ID"));
  assert.notEqual(constant("PROD_EXTENSION_ID"), constant("DEV_EXTENSION_ID"));
});

test("host names in Rust match the extension scripts", () => {
  assert.equal(constant("PROD_HOST_NAME"), PROD_HOST);
  assert.equal(constant("DEV_HOST_NAME"), DEV_HOST);
  assert.match(readFileSync(new URL("./background.js", import.meta.url), "utf8"), new RegExp(`HOST = "${PROD_HOST}"`));
});

test("extension keeps minimal permissions", () => {
  assert.deepEqual([...manifest.permissions].sort(), ["contextMenus", "nativeMessaging"]);
  assert.equal(manifest.host_permissions, undefined);
  assert.equal(manifest.content_scripts, undefined);
});

test("generated dev extension: own ID and host, same permissions, no leftovers", () => {
  const out = join(mkdtempSync(join(tmpdir(), "pinit-devext-")), "ext");
  buildDevExtension(fileURLToPath(new URL(".", import.meta.url)), out, dev);
  const m = JSON.parse(readFileSync(join(out, "manifest.json"), "utf8"));
  const bg = readFileSync(join(out, "background.js"), "utf8");
  assert.equal(idFromKey(m.key), dev.id);
  assert.deepEqual(m.permissions, manifest.permissions);
  assert.match(bg, new RegExp(`HOST = "${DEV_HOST}"`));
  assert.ok(!bg.includes(`HOST = "${PROD_HOST}"`));
  assert.ok(bg.includes("PinIt DEV로 열기"));
});
