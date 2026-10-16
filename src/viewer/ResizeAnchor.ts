import type { Stage } from "./Stage.ts";
import { anchorOnResize, type Box } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** Ignore pointer moves right after a grip press: the OS resize loop has not started yet. */
const SETTLE_MS = 300;

/**
 * While the user drags a window edge, the picture stays put on screen at the same scale and
 * only the window (the mask) grows or shrinks. See `anchorOnResize` for the math; it needs
 * only which edge was grabbed and the window sizes seen by the ResizeObserver.
 */
export class ResizeAnchor {
  private grip: { west: boolean; north: boolean } | null = null;
  private startedAt = 0;
  private last: Box = { w: 0, h: 0 };
  private lastFit = 0;

  constructor(private readonly stage: Stage, private readonly store: ViewStore) {
    new ResizeObserver(() => this.apply()).observe(stage.el);
    // The OS resize loop swallows pointer events; the first one afterwards ends the drag.
    const end = () => {
      if (this.grip && performance.now() - this.startedAt > SETTLE_MS) this.grip = null;
    };
    window.addEventListener("pointermove", end, true);
    window.addEventListener("pointerup", end, true);
  }

  /** Call on grip press (`dir` e.g. "NorthWest"), before the OS resize starts. */
  begin(dir: string): void {
    this.grip = { west: dir.includes("West"), north: dir.includes("North") };
    this.startedAt = performance.now();
    this.last = this.stage.windowBox();
    this.lastFit = this.stage.fitScale(this.last);
  }

  private apply(): void {
    if (!this.grip) return;
    const win = this.stage.windowBox();
    const fit = this.stage.fitScale(win);
    if (win.w === 0 || win.h === 0 || !fit || !this.lastFit) return;
    const [from, fitFrom] = [this.last, this.lastFit];
    [this.last, this.lastFit] = [win, fit];
    this.store.update((v) => anchorOnResize(v, from, win, fitFrom, fit, this.grip!));
  }
}
