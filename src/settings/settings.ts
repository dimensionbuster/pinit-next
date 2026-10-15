import { listen } from "@tauri-apps/api/event";
import "../design.css";
import "./settings.css";
import { h } from "../ui/dom.ts";
import { SettingsModel } from "./SettingsModel.ts";
import { AppearanceSection, ArrangeSection, BrowserSection, Footer, Header, LayoutSection, UpdateBanner } from "./sections.ts";
import { toast } from "./widgets.ts";

async function main(): Promise<void> {
  const model = await SettingsModel.load();
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");

  const sections = [
    new UpdateBanner().init(),
    new Header(model).init(),
    new ArrangeSection(model).init(),
    new LayoutSection(model).init(),
    new AppearanceSection(model).init(),
    new BrowserSection(model).init(),
    new Footer(),
  ];
  root.append(h("main", { class: "page" }, ...sections.map((s) => s.el)));

  // Keep the open-window count fresh when the window regains focus.
  window.addEventListener("focus", () => void model.refresh());

  await listen<string>("toast", (e) => toast(e.payload));
  await listen("bridge-connected", () => {
    void model.reloadSettings();
    toast("브라우저 확장이 연결되었습니다");
  });
}

main().catch((e) => {
  document.body.textContent = String(e);
});
