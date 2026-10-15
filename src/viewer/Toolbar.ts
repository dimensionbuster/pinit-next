import { ipc } from "../ipc.ts";
import { h } from "../ui/dom.ts";
import { icon, type IconName } from "../ui/icons.ts";
import { defaultView, rotateQuarter, zoomAt } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

function button(name: IconName, title: string, onClick: () => void, extra = ""): HTMLButtonElement {
  return h("button", { class: `tb-btn ${extra}`.trim(), title, "aria-label": title, onClick }, icon(name));
}

/** Hover overlay: close (top-right), tool pill (bottom), free-angle chip. */
export class Toolbar {
  readonly el: HTMLElement;

  constructor(store: ViewStore) {
    const close = button("close", "닫기", () => void ipc.viewerClose(), "close");

    const tools = h(
      "div",
      { class: "pill", role: "toolbar", "aria-label": "이미지 도구" },
      button("rotateCcw", "왼쪽으로 90° 회전 (Shift+R)", () => store.update((v) => rotateQuarter(v, false))),
      button("rotateCw", "오른쪽으로 90° 회전 (R)", () => store.update((v) => rotateQuarter(v, true))),
      button("flipH", "좌우 반전 (H)", () => store.update((v) => ({ ...v, flip_h: !v.flip_h }))),
      button("flipV", "상하 반전 (V)", () => store.update((v) => ({ ...v, flip_v: !v.flip_v }))),
      h("span", { class: "sep" }),
      button("zoomOut", "축소 (-)", () => store.update((v) => zoomAt(v, 1 / 1.2, { x: 0, y: 0 }))),
      button("zoomIn", "확대 (+)", () => store.update((v) => zoomAt(v, 1.2, { x: 0, y: 0 }))),
      button("reset", "원래대로 (0)", () => store.set(defaultView())),
      h("span", { class: "sep" }),
      button("download", "다른 이름으로 저장", () => void ipc.viewerSaveAs()),
      button("external", "원본 열기", () => void ipc.viewerOpenSource()),
    );

    const chip = h("button", {
      class: "angle-chip",
      title: "자유 회전 초기화 (Alt+휠로 조절)",
      hidden: true,
      onClick: () => store.update((v) => ({ ...v, fine_angle_deg: 0 })),
    });

    store.onChange((v) => {
      const a = v.fine_angle_deg;
      chip.hidden = Math.abs(a) < 0.05;
      chip.textContent = `${a.toFixed(0)}°`;
    });

    this.el = h("div", { class: "overlay" }, close, chip, tools);
  }
}
