import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "../design.css";
import "./viewer.css";
import { ipc, type Settings } from "../ipc.ts";
import { dbg } from "./debug.ts";
import { h } from "../ui/dom.ts";
import { KeyboardController, PointerController } from "./controllers.ts";
import { ResizeAnchor } from "./ResizeAnchor.ts";
import { createResizeGrips } from "./ResizeGrips.ts";
import { Stage } from "./Stage.ts";
import { Toolbar } from "./Toolbar.ts";
import { backdropCss } from "./transform.ts";
import { ViewStore } from "./ViewStore.ts";

async function main(): Promise<void> {
  const init = await ipc.viewerInit();
  const store = new ViewStore(init.view, (v) => void ipc.viewerSetView(v));
  const stage = new Stage(convertFileSrc(init.path), store);

  if (init.stream) {
    stage.enableStream();
    // This window's own listener: a global `listen` also receives frames sent to other windows.
    await getCurrentWebviewWindow().listen<string>("video-frame", (e) => stage.pushFrame(e.payload));
  }

  new PointerController(stage, store);
  new KeyboardController(store);

  const applyBackdrop = (s: Settings) => stage.setBackdrop(backdropCss(s.backdrop_color, s.backdrop_opacity));
  applyBackdrop(await ipc.getSettings());
  await listen<Settings>("settings-changed", (e) => applyBackdrop(e.payload));

  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");
  root.append(stage.el, new Toolbar(store).el, h("div", { class: "grips" }, ...createResizeGrips()));
  new ResizeAnchor(root, stage, store, init.margin);
}

window.addEventListener("error", (e) => dbg(`error: ${e.message} @${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => dbg(`unhandled rejection: ${String(e.reason)}`));

main().catch((e) => {
  document.body.textContent = String(e);
});
