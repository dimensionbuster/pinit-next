import { listen } from "@tauri-apps/api/event";
import { ipc } from "../ipc.ts";
import { dbg, fmtRect } from "./debug.ts";
import type { ViewState } from "../ipc.ts";
import type { Stage } from "./Stage.ts";
import { anchorOnFrame, isTouched, type Frame } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/**
 * `win-frame` from the host (physical px). `keep`: the surface held its place on screen.
 * `sx/sy`: how far the surface origin moved on screen (only when it grew past its margin).
 */
interface FrameEvent extends Frame {
  keep: boolean;
  sx: number;
  sy: number;
}

/**
 * For a window the user zoomed or rotated, the picture is a layer fixed on screen and the window
 * is a mask over it: while the user drags an edge the picture keeps its place and scale, only
 * the mask edge moves. A window never touched (or reset) lets the picture follow its size.
 *
 * The host (`anchor.rs`) does the moving: it keeps the page surface still on screen, in the very
 * message that resizes the window, so nothing can lag. All that is left for the page is to say
 * which windows are pinned and to follow the frame (the window's place inside the surface):
 * `#root` is laid out at the frame, and the view is re-expressed so the picture stays put.
 */
export class ResizeAnchor {
  private frame: Frame;
  private pinned = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly stage: Stage,
    private readonly store: ViewStore,
    marginPx: number,
  ) {
    // Until the first event the window sits at the surface margin and reaches its far edges.
    const m = marginPx / window.devicePixelRatio;
    root.style.inset = `${m}px 0 0 ${m}px`;
    this.frame = { x: m, y: m, ...stage.windowBox() };
    void listen<FrameEvent>("win-frame", (e) => this.onFrame(e.payload));
    store.onChange((v) => this.syncPin(v));
    this.syncPin(store.get());
    dbg(`init margin=${marginPx}px dpr=${window.devicePixelRatio} inner=${window.innerWidth}x${window.innerHeight} root=${fmtRect(root)} stage=${fmtRect(stage.el)}`);
  }

  private syncPin(v: ViewState): void {
    const touched = isTouched(v);
    if (touched === this.pinned) return;
    this.pinned = touched;
    dbg(`pin -> ${touched}  view zoom=${v.zoom.toFixed(3)} q=${v.quarter_turns} fine=${v.fine_angle_deg.toFixed(1)}`);
    void ipc.viewerPin(touched);
  }

  private onFrame(e: FrameEvent): void {
    const dpr = window.devicePixelRatio;
    const prev = this.frame;
    const next = { x: e.x / dpr, y: e.y / dpr, w: e.w / dpr, h: e.h / dpr };
    this.frame = next;
    const s = this.root.style;
    s.inset = "auto";
    s.left = `${next.x}px`;
    s.top = `${next.y}px`;
    s.width = `${next.w}px`;
    s.height = `${next.h}px`;
    const before = this.store.get();
    const compensate = e.keep && isTouched(before);
    if (compensate) {
      const origin = { x: e.sx / dpr, y: e.sy / dpr };
      this.store.set(
        anchorOnFrame(before, this.stage.fitScale(prev), prev, this.stage.fitScale(next), next, origin),
      );
    }
    const after = this.store.get();
    const f = (n: number) => n.toFixed(1);
    dbg(
      `frame phys=(${e.x},${e.y} ${e.w}x${e.h}) keep=${e.keep} origin=(${e.sx},${e.sy}) compensate=${compensate}` +
        `  css=(${f(next.x)},${f(next.y)} ${f(next.w)}x${f(next.h)})` +
        `  zoom ${before.zoom.toFixed(3)}->${after.zoom.toFixed(3)} pan (${f(before.pan_x)},${f(before.pan_y)})->(${f(after.pan_x)},${f(after.pan_y)})` +
        `  | dom: inner=${window.innerWidth}x${window.innerHeight} root=${fmtRect(this.root)} stage=${fmtRect(this.stage.el)}`,
    );
  }
}
