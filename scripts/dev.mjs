// `npm run dev:app`: refresh the dev extension, then start the isolated debug app with hot reload.
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

const gen = spawnSync(process.execPath, ["scripts/dev-extension.mjs"], { cwd: root, stdio: "inherit" });
if (gen.status !== 0) process.exit(gen.status ?? 1);

const app = spawn("npx", ["tauri", "dev", "-c", "src-tauri/tauri.dev.conf.json"], {
  cwd: root,
  stdio: "inherit",
  shell: true,
});
app.on("exit", (code) => process.exit(code ?? 0));
