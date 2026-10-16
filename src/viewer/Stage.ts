import { h } from "../ui/dom.ts";
import { cssTransform, fitScale, imageBox, type Box } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** Renders the image and keeps its transform in sync with the store and window size. */
export class Stage {
  readonly el: HTMLDivElement;
  private readonly img: HTMLImageElement;
  private readonly status: HTMLDivElement;
  /** What is painted: the image, the canvas of a live video, or a local video. */
  private surface: HTMLImageElement | HTMLCanvasElement | HTMLVideoElement;
  private canvas: HTMLCanvasElement | null = null;
  private video: HTMLVideoElement | null = null;
  private decoding = false;
  private pending: string | null = null;

  constructor(src: string, private readonly store: ViewStore) {
    this.img = h("img", { class: "image", alt: "", draggable: false });
    this.status = h("div", { class: "status", hidden: true });
    this.el = h("div", { class: "stage" }, this.img, this.status);
    this.surface = this.img;

    this.img.addEventListener("error", () => {
      if (!this.video && !this.canvas) this.showStatus("이미지를 불러올 수 없습니다");
    });
    this.img.src = src;

    new ResizeObserver(() => this.render()).observe(this.el);
    store.onChange(() => this.render());
    this.render();
  }

  /**
   * Paint the area behind/around the image (visible when panned, zoomed out or letterboxed). It is
   * the page background, not the stage's, so it covers the whole surface the window clips.
   */
  setBackdrop(css: string): void {
    document.body.style.background = css;
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

  /**
   * Local video file: a `<video>` takes the image's place and plays natively (full quality,
   * sound). `onSize` gets the real dimensions once the metadata is known.
   */
  enableVideo(src: string, onSize: (w: number, h: number) => void): void {
    const video = h("video", { class: "image", loop: true, playsInline: true });
    video.src = src;
    this.video = video;
    this.surface = video;
    this.img.removeAttribute("src"); // the file is a video: the <img> must not try to load it
    this.img.hidden = true;
    this.el.insertBefore(video, this.status);
    video.addEventListener("loadedmetadata", () => {
      onSize(video.videoWidth, video.videoHeight);
      this.render();
    });
    video.addEventListener("error", () => this.showStatus("이 영상은 재생할 수 없습니다 (코덱 미지원일 수 있어요)"));
    // sound on if the webview allows autoplay with audio; otherwise start muted
    video.play().catch(() => {
      video.muted = true;
      void video.play().catch(() => {});
    });
  }

  togglePlay(): void {
    const v = this.video;
    if (!v) return;
    if (v.paused) void v.play().catch(() => {});
    else v.pause();
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

  windowBox(): Box {
    return { w: this.el.clientWidth, h: this.el.clientHeight };
  }

  /** Fit scale (unzoomed image vs window); 0 until the image has loaded. */
  fitScale(win: Box = this.windowBox()): number {
    const natural = this.video
      ? { w: this.video.videoWidth, h: this.video.videoHeight }
      : this.canvas
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
    this.surface.style.width = `${box.w}px`;
    this.surface.style.height = `${box.h}px`;
    this.surface.style.transform = cssTransform(v);
  }
}
