import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "../design.css";
import "./viewer.css";
import { ipc } from "../ipc.ts";
import { h } from "../ui/dom.ts";
import { KeyboardController, PointerController } from "./controllers.ts";
import { createResizeGrips } from "./ResizeGrips.ts";
import { Stage } from "./Stage.ts";
import { Toolbar } from "./Toolbar.ts";
import { ViewStore } from "./ViewStore.ts";

async function main(): Promise<void> {
  const init = await ipc.viewerInit();
  const store = new ViewStore(init.view, (v) => void ipc.viewerSetView(v));
  const stage = new Stage(convertFileSrc(init.path), store);

  new PointerController(stage, store);
  new KeyboardController(store);

  // After the user resizes, snap the window back to the image's aspect ratio.
  let fitTimer: ReturnType<typeof setTimeout> | undefined;
  void getCurrentWindow().onResized(() => {
    clearTimeout(fitTimer);
    fitTimer = setTimeout(() => void ipc.viewerFit(), 250);
  });

  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");
  root.append(stage.el, new Toolbar(store).el, h("div", { class: "grips" }, ...createResizeGrips()));
}

main().catch((e) => {
  document.body.textContent = String(e);
});
