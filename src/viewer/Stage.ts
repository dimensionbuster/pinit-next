import { h } from "../ui/dom.ts";
import { cssTransform, fitScale, imageBox, type Box } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** Renders the image and keeps its transform in sync with the store and window size. */
export class Stage {
  readonly el: HTMLDivElement;
  private readonly img: HTMLImageElement;
  private readonly status: HTMLDivElement;
  /** What is painted: the image, or the canvas of a live video. */
  private surface: HTMLImageElement | HTMLCanvasElement;
  private canvas: HTMLCanvasElement | null = null;
  private decoding = false;
  private pending: string | null = null;
  private reported: Box | null = null;

  constructor(src: string, private readonly store: ViewStore) {
    this.img = h("img", { class: "image", alt: "", draggable: false });
    this.status = h("div", { class: "status", hidden: true });
    this.el = h("div", { class: "stage" }, this.img, this.status);
    this.surface = this.img;

    this.img.addEventListener("error", () => this.showStatus("이미지를 불러올 수 없습니다"));
    this.img.src = src;

    new ResizeObserver(() => this.render()).observe(this.el);
    store.onChange(() => this.render());
    this.render();
  }

  /** Paint the area behind/around the image (visible when panned, zoomed out or letterboxed). */
  setBackdrop(css: string): void {
    this.el.style.background = css;
  }

  /**
   * Live video: draw incoming JPEG frames (base64) on a canvas that takes the image's
   * place. Only the newest frame is kept while one is still decoding.
   */
  enableStream(): void {
    const canvas = h("canvas", { class: "image" });
    this.canvas = canvas;
    this.surface = canvas;
    this.img.hidden = true;
    this.el.insertBefore(canvas, this.status);
  }

  pushFrame(b64: string): void {
    if (!this.canvas) return;
    if (this.decoding) {
      this.pending = b64;
      return;
    }
    this.decoding = true;
    void this.drawFrame(b64).finally(() => {
      this.decoding = false;
      const next = this.pending;
      this.pending = null;
      if (next) this.pushFrame(next);
    });
  }

  private async drawFrame(b64: string): Promise<void> {
    const canvas = this.canvas;
    if (!canvas) return;
    try {
      const bin = atob(b64);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/jpeg" }));
      if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
        canvas.width = bitmap.width;
        canvas.height = bitmap.height;
        this.render();
      }
      canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
      bitmap.close();
    } catch {
      // a corrupt frame is skipped; the next one repaints
    }
  }

  /**
   * Window size as reported by the host. It is exact and leads the DOM, which lags the window
   * by a frame while it is resized; the picture is positioned from it, not from the layout.
   */
  setWindowBox(box: Box): void {
    this.reported = box;
    this.render();
  }

  windowBox(): Box {
    return this.reported ?? { w: this.el.clientWidth, h: this.el.clientHeight };
  }

  /** Fit scale (unzoomed image vs window); 0 until the image has loaded. */
  fitScale(win: Box = this.windowBox()): number {
    const natural = this.canvas
      ? { w: this.canvas.width, h: this.canvas.height }
      : { w: this.img.naturalWidth, h: this.img.naturalHeight };
    return fitScale(win, this.store.get().quarter_turns, natural);
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
    this.surface.style.left = `${win.w / 2}px`;
    this.surface.style.top = `${win.h / 2}px`;
    this.surface.style.width = `${box.w}px`;
    this.surface.style.height = `${box.h}px`;
    this.surface.style.transform = cssTransform(v);
  }
}
