import type { ViewState } from "../ipc.ts";
import type { Stage } from "./Stage.ts";
import { anchorOnResize, type Box, type DragEdges } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

interface Base {
  view: ViewState;
  fit: number;
  win: Box;
  edges: DragEdges;
}

/** Ignore pointer moves right after a grip press: the OS resize loop has not started yet. */
const SETTLE_MS = 300;

/**
 * While the user drags a window edge, the picture stays put on screen at the same scale and
 * only the window (the mask) grows or shrinks. Uses just the window size and the dragged
 * edges (see `anchorOnResize`), so there is no screen-position tracking to go stale.
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

  /** Call on grip press with the grip direction, before the OS resize starts. */
  begin(dir: string): void {
    const win = this.stage.windowBox();
    this.base = {
      view: this.store.get(),
      fit: this.stage.fitScale(win),
      win,
      edges: { west: dir.includes("West"), north: dir.includes("North") },
    };
    this.startedAt = performance.now();
  }

  private end(): void {
    if (this.base && performance.now() - this.startedAt > SETTLE_MS) this.base = null;
  }

  private apply(): void {
    const b = this.base;
    const win = this.stage.windowBox();
    if (!b || win.w === 0 || win.h === 0) return;
    this.store.set(anchorOnResize(b.view, b.fit, b.win, this.stage.fitScale(win), win, b.edges));
  }
}
