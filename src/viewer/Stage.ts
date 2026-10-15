import { h } from "../ui/dom.ts";
import { cssTransform, imageBox, type Box } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** Renders the image and keeps its transform in sync with the store and window size. */
export class Stage {
  readonly el: HTMLDivElement;
  private readonly img: HTMLImageElement;
  private readonly status: HTMLDivElement;

  constructor(src: string, private readonly store: ViewStore) {
    this.img = h("img", { class: "image", alt: "", draggable: false });
    this.status = h("div", { class: "status", hidden: true });
    this.el = h("div", { class: "stage" }, this.img, this.status);

    this.img.addEventListener("error", () => this.showStatus("이미지를 불러올 수 없습니다"));
    this.img.src = src;

    new ResizeObserver(() => this.render()).observe(this.el);
    store.onChange(() => this.render());
    this.render();
  }

  windowBox(): Box {
    return { w: this.el.clientWidth, h: this.el.clientHeight };
  }

  private showStatus(text: string): void {
    this.status.textContent = text;
    this.status.hidden = false;
    this.img.hidden = true;
  }

  private render(): void {
    const win = this.windowBox();
    if (win.w === 0 || win.h === 0) return;
    const v = this.store.get();
    const box = imageBox(win, v.quarter_turns);
    this.img.style.width = `${box.w}px`;
    this.img.style.height = `${box.h}px`;
    this.img.style.transform = cssTransform(v, win);
  }
}
