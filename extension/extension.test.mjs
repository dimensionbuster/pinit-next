// Guards the invariant the native host relies on: the fixed extension ID.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const manifest = JSON.parse(readFileSync(new URL("./manifest.json", import.meta.url), "utf8"));
const rust = readFileSync(new URL("../src-tauri/src/native_host.rs", import.meta.url), "utf8");

function idFromKey(b64) {
  const hash = createHash("sha256").update(Buffer.from(b64, "base64")).digest().subarray(0, 16);
  return [...hash].map((b) => String.fromCharCode(97 + (b >> 4)) + String.fromCharCode(97 + (b & 15))).join("");
}

test("manifest key derives the extension id hard-coded in native_host.rs", () => {
  const declared = /EXTENSION_ID: &str = "([a-p]{32})"/.exec(rust)?.[1];
  assert.equal(idFromKey(manifest.key), declared);
});

test("extension keeps minimal permissions", () => {
  assert.deepEqual([...manifest.permissions].sort(), ["contextMenus", "nativeMessaging"]);
  assert.equal(manifest.host_permissions, undefined);
  assert.equal(manifest.content_scripts, undefined);
});
