# Dependency budget

New dependency = one line here with the reason. Default answer is no.

## Rust
| crate | where | reason |
|---|---|---|
| tauri 2 | src-tauri | window/tray/webview host |
| tauri-plugin-opener | src-tauri | scaffold default; remove if unused |
| serde, serde_json | src-tauri | IPC + persistence |

## JS (dev only, zero runtime deps target)
| package | reason |
|---|---|
| typescript, vite | build |
| @tauri-apps/api | IPC bridge |
