import type { Stage } from "./Stage.ts";
import { anchorOnResize, type Box } from "./transform.ts";
import type { ViewState } from "../ipc.ts";
import type { ViewStore } from "./ViewStore.ts";

interface Base {
  view: ViewState;
  fit: number;
  c: { x: number; y: number };
}

/** Ignore pointer moves right after a grip press: the OS resize loop has not started yet. */
const SETTLE_MS = 300;

/**
 * While the user drags a window edge, the picture stays put on screen at the same
 * scale and only the window (the mask) grows or shrinks. Resizing changes the
 * window centre and the contain-fit scale; both are compensated through the view.
 * Screen position comes from `window.screenX/Y`, read synchronously in the same
 * frame as the resize so the picture never lags behind the mask.
 */
export class ResizeAnchor {
  private base: Base | null = null;
  private startedAt = 0;

  constructor(private readonly stage: Stage, private readonly store: ViewStore) {
    new ResizeObserver(() => this.apply()).observe(stage.el);
    // The OS resize loop swallows pointer events; the first one afterwards ends the drag.
    window.addEventListener("pointermove", () => this.end(), true);
    window.addEventListener("pointerup", () => this.end(), true);
  }

  /** Call on grip press, before the OS resize starts. */
  begin(): void {
    const win = this.stage.windowBox();
    const fit = this.stage.fitScale(win);
    if (!fit) return;
    this.base = { view: this.store.get(), fit, c: this.center(win) };
    this.startedAt = performance.now();
  }

  private end(): void {
    if (this.base && performance.now() - this.startedAt > SETTLE_MS) this.base = null;
  }

  private center(win: Box): { x: number; y: number } {
    return { x: window.screenX + win.w / 2, y: window.screenY + win.h / 2 };
  }

  private apply(): void {
    const b = this.base;
    if (!b) return;
    const win = this.stage.windowBox();
    if (win.w === 0 || win.h === 0) return;
    const fit = this.stage.fitScale(win);
    if (!fit) return;
    this.store.set(anchorOnResize(b.view, b.fit, b.c, fit, this.center(win)));
  }
}
