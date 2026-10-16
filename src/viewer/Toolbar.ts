import { ipc } from "../ipc.ts";
import { h } from "../ui/dom.ts";
import { icon, type IconName } from "../ui/icons.ts";
import { defaultView, flipScreen, rotateQuarter, zoomAt } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

function button(name: IconName, title: string, onClick: () => void, extra = ""): HTMLButtonElement {
  return h("button", { class: `tb-btn ${extra}`.trim(), title, "aria-label": title, onClick }, icon(name));
}

/** Window short side (CSS px) under which the tools collapse behind a button. */
const COMPACT_PX = 240;
/** Cursor distance from the bottom edge that reveals the tool pill. */
const NEAR_BOTTOM_PX = 56;

/** Hover overlay: close (top-right), tool pill (bottom), free-angle chip. */
export class Toolbar {
  readonly el: HTMLElement;

  constructor(store: ViewStore) {
    const close = button("close", "닫기", () => void ipc.viewerClose(), "close");

    const tools = h(
      "div",
      { class: "pill", role: "toolbar", "aria-label": "이미지 도구" },
      button("rotateCcw", "왼쪽으로 90° 회전", () => store.update((v) => rotateQuarter(v, false))),
      button("rotateCw", "오른쪽으로 90° 회전", () => store.update((v) => rotateQuarter(v, true))),
      button("flipH", "좌우 반전 (H)", () => store.update((v) => flipScreen(v, "h"))),
      button("flipV", "상하 반전 (V)", () => store.update((v) => flipScreen(v, "v"))),
      h("span", { class: "sep" }),
      button("zoomOut", "축소 (-)", () => store.update((v) => zoomAt(v, 1 / 1.2, { x: 0, y: 0 }))),
      button("zoomIn", "확대 (+)", () => store.update((v) => zoomAt(v, 1.2, { x: 0, y: 0 }))),
      button("reset", "원래대로 (0)", () => store.set(defaultView())),
      button("fit", "창을 이미지 비율에 맞춤", () => void ipc.viewerFit()),
      h("span", { class: "sep" }),
      button("download", "다른 이름으로 저장", () => void ipc.viewerSaveAs()),
      button("external", "원본 열기", () => void ipc.viewerOpenSource()),
    );

    const chip = h("button", {
      class: "angle-chip",
      title: "자유 회전 초기화 (R을 누른 채 끌어서 조절)",
      hidden: true,
      onClick: () => store.update((v) => ({ ...v, fine_angle_deg: 0 })),
    });

    store.onChange((v) => {
      const a = v.fine_angle_deg;
      chip.hidden = Math.abs(a) < 0.05;
      chip.textContent = `${a.toFixed(0)}°`;
    });

    // Small windows: the tool pill collapses behind a "more" button so it cannot cover the picture.
    const more = button("more", "도구 펼치기", () => this.el.classList.toggle("expanded"), "more");

    this.el = h("div", { class: "overlay" }, close, chip, more, tools);
    this.watchPointer();
  }

  /**
   * Keeps the pill out of the way: it only shows while the cursor is near the bottom edge
   * (or over the pill). Below COMPACT_PX it stays hidden until "more" is pressed.
   */
  private watchPointer(): void {
    const root = document.documentElement;
    // the window is the frame (#root = the overlay's parent), not the page surface around it
    const frame = () => this.el.getBoundingClientRect();
    const setNear = (near: boolean) => this.el.classList.toggle("near-bottom", near);
    const measure = () => {
      const f = frame();
      this.el.classList.toggle("compact", Math.min(f.width, f.height) < COMPACT_PX);
    };
    new ResizeObserver(measure).observe(this.el);
    document.addEventListener("pointermove", (e) => setNear(e.clientY > frame().bottom - NEAR_BOTTOM_PX));
    root.addEventListener("mouseleave", () => {
      setNear(false);
      this.el.classList.remove("expanded");
    });
  }
}
