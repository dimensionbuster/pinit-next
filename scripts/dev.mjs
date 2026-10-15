// `npm run dev:app`: refresh the dev extension, then start the isolated debug app with hot reload.
import { spawn, spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

const gen = spawnSync(process.execPath, ["scripts/dev-extension.mjs"], { cwd: root, stdio: "inherit" });
if (gen.status !== 0) process.exit(gen.status ?? 1);

// Port: `npm run dev:app -- --port 1430` or PINIT_DEV_PORT=1430 (default 1420). Lets two checkouts/runs coexist.
const flag = process.argv.indexOf("--port");
const port = Number(flag > 0 ? process.argv[flag + 1] : process.env.PINIT_DEV_PORT) || 1420;
const configs = ["src-tauri/tauri.dev.conf.json"];
if (port !== 1420) {
  const override = join(tmpdir(), `pinit-dev-${port}.conf.json`);
  writeFileSync(override, JSON.stringify({ build: { devUrl: `http://localhost:${port}` } }));
  configs.push(override);
}

const app = spawn("npx", ["tauri", "dev", ...configs.flatMap((c) => ["-c", `"${c}"`])], {
  cwd: root,
  stdio: "inherit",
  shell: true,
  env: { ...process.env, PINIT_DEV_PORT: String(port) },
});
// `tauri dev` leaves its vite child alive when the app quits or the terminal is interrupted, and the
// stale vite then blocks the port on the next run. Whatever listens on our port at exit is ours
// (a pre-existing listener would have failed vite's strictPort at startup), so stop the tree and it.
function cleanup() {
  if (app.pid) spawnSync("taskkill", ["/pid", String(app.pid), "/T", "/F"], { stdio: "ignore" });
  const out = spawnSync("netstat", ["-ano", "-p", "tcp"], { encoding: "utf8" }).stdout ?? "";
  for (const line of out.split(/\r?\n/)) {
    const cols = line.trim().split(/\s+/);
    if (cols[3] === "LISTENING" && cols[1]?.endsWith(`:${port}`) && /^\d+$/.test(cols[4]) && cols[4] !== "0") {
      spawnSync("taskkill", ["/pid", cols[4], "/T", "/F"], { stdio: "ignore" });
    }
  }
}

app.on("exit", (code) => {
  cleanup();
  process.exit(code ?? 0);
});
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP", "SIGBREAK"]) {
  process.on(sig, () => {
    cleanup();
    process.exit(0);
  });
}
