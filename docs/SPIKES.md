# Phase 1 spike results

Machine: Windows 11, 2 monitors — DISPLAY2 3840x2160 @ scale 1.5 at (0,0);
DISPLAY1 1920x1080 @ scale 1.0 at (3840,483). Tauri 2.12 (debug build), WebView2 155, Electron 33.
All windows: 300x300 DIP, transparent, frameless, always-on-top, same origin, idle, 10 s after start.

## 1. Memory: N transparent windows

| runtime | windows | processes | working set | private |
|---|---|---|---|---|
| Electron 33 | 1 | 4 | 303 MB | 176 MB |
| Electron 33 | 20 | 23 | 1626 MB | 706 MB |
| Tauri (default WebView2) | 1 | 7 | 325 MB | 141 MB |
| Tauri (default WebView2) | 20 | 26 | 1309 MB | 1049 MB |
| Tauri + `--renderer-process-limit=1` | 20 | 26 | 1300 MB | 1053 MB (no effect) |
| **Tauri + `--process-per-site`** | 20 | **7** | **410 MB** | 692 MB |

Findings
- With default settings Tauri is **not** a memory win over Electron (~48 MB/window marginal;
  private bytes even higher). The earlier "much smaller per window" assumption was wrong.
- `--process-per-site` makes all same-origin windows share one renderer: 20 windows in 7
  processes, working set 410 MB (3x lower than default, 4x lower than Electron).
  Set via `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` or identical `additional_browser_args` on
  every window **including the main window** (mixed args => `0x8007139F` on webview creation).
- Trade-off: one hung/crashed renderer affects every image window. Acceptable for a viewer.
- Remaining benefits of Tauri over Electron: installer size, Rust core with compiler-enforced
  dependency direction, no bundled Chromium. Runtime adapters stay swappable (ports).

## 2. DPI / mixed-monitor geometry

- Tauri reports monitor size/position in **physical** pixels; each monitor has its own scale.
- A 300x300 logical window created on the 1.5x monitor has inner_size 450x450 physical.
- Virtual desktop is linear only in physical pixels, so core geometry is **physical px**; DIP
  settings (grid line, jigsaw base) are multiplied by the target display's scale at placement
  time (`LayoutConfig::strategy(scale)`). Adapter must use `PhysicalPosition`/`PhysicalSize`.

## 3. Windows build gotcha

Extra Cargo `examples` do not get the Windows manifest (comctl32 v6) and crash with
`0xC0000139`. Spike binaries must be `[[bin]]` targets of the Tauri package.

## Still open
- Native Messaging: extension -> host bytes path (same exe with `--native-host`).
- Drag and free-angle rotation behavior inside transparent windows.
