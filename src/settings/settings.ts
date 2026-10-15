import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import "../design.css";
import "./settings.css";
import { ipc } from "../ipc.ts";
import { h } from "../ui/dom.ts";
import { SettingsModel } from "./SettingsModel.ts";
import { AppearanceSection, ArrangeSection, BrowserSection, Header, LayoutSection, OpenSection } from "./sections.ts";
import { toast } from "./widgets.ts";

async function main(): Promise<void> {
  const model = await SettingsModel.load();
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");

  const dropHint = h("div", { class: "drop-overlay", hidden: true }, "놓아서 열기");
  const sections = [
    new Header(model).init(),
    new OpenSection(model).init(),
    new ArrangeSection(model).init(),
    new LayoutSection(model).init(),
    new AppearanceSection(model).init(),
    new BrowserSection(model).init(),
  ];
  root.append(h("main", { class: "page" }, ...sections.map((s) => s.el)), dropHint);

  // Keep the open-window count fresh when the window regains focus.
  window.addEventListener("focus", () => void model.refresh());

  await listen<string>("toast", (e) => toast(e.payload));
  await getCurrentWebview().onDragDropEvent((e) => {
    const t = e.payload.type;
    if (t === "enter" || t === "over") dropHint.hidden = false;
    else dropHint.hidden = true;
    if (t === "drop") {
      void ipc.openPaths(e.payload.paths).then((n) => {
        if (n === 0) toast("열 수 있는 이미지가 없습니다");
        return model.refresh();
      });
    }
  });
}

main().catch((e) => {
  document.body.textContent = String(e);
});
