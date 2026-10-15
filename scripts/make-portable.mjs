// Builds the standalone exe and assembles a portable folder:
//   release-portable/PinIt/{PinIt.exe, portable.flag, README.txt}
// Usage: npm run build:portable
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const exe = join(root, "target", "release", "pinit-next.exe");
const out = join(root, "release-portable", "PinIt");

const build = spawnSync("npx", ["tauri", "build", "--no-bundle"], { cwd: root, stdio: "inherit", shell: true });
if (build.status !== 0 || !existsSync(exe)) {
  console.error("build failed");
  process.exit(build.status ?? 1);
}

// If the previous build is still running (exe locked), write a timestamped folder instead.
let target = out;
try {
  rmSync(out, { recursive: true, force: true });
} catch {
  target = `${out}-${new Date().toISOString().replace(/[:.]/g, "-")}`;
  console.warn(`${out} is in use (PinIt running?) -> writing to ${target}`);
}
mkdirSync(target, { recursive: true });
copyFileSync(exe, join(target, "PinIt.exe"));
writeFileSync(join(target, "portable.flag"), "");
writeFileSync(
  join(target, "README.txt"),
  [
    "PinIt portable",
    "",
    "- PinIt.exe 를 아무 폴더에서 실행하세요. 설치가 필요 없습니다.",
    "- portable.flag 가 exe 옆에 있는 동안 모든 설정/캐시는 PinIt-data 폴더에만 저장됩니다.",
    "  (AppData에는 아무것도 쓰지 않습니다. 폴더째 옮기거나 지우면 흔적이 사라집니다.)",
    "- 브라우저 이미지 우클릭 연동을 쓰면 현재 사용자 레지스트리(HKCU)에 연결 정보가 등록됩니다.",
    "  원치 않으면 확장 프로그램을 설치하지 마세요.",
    "- Windows 10에서 실행되지 않으면 Microsoft Edge WebView2 Runtime 설치가 필요합니다.",
    "",
  ].join("\r\n"),
);
console.log("portable folder:", target);
