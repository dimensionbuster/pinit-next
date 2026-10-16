import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import type { Stage } from "./Stage.ts";
import { anchorOnResize, draggedEdges, isTouched, type Rect } from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** `win-geometry` from Rust: the window's rect (physical px) and whether the mouse button is down. */
interface Geometry extends Rect {
  user: boolean;
}

/**
 * For a window the user zoomed or rotated, the picture is a layer fixed on screen and the window
 * is a mask over it: while the user drags an edge the picture keeps its place and scale, only
 * the mask edge moves. A window never touched (or reset) just lets the picture follow its size.
 *
 * Rust reports every window rect change. The previous and new rect tell which edges moved
 * (`draggedEdges`), and the sizes come from these rects, never from the DOM (which lags the
 * window by a frame). The same sizes are handed to the stage for rendering.
 */
export class ResizeAnchor {
  private prev: Rect | null = null;

  constructor(private readonly stage: Stage, private readonly store: ViewStore) {
    void listen<Geometry>("win-geometry", (e) => this.onGeometry(e.payload));
    const win = getCurrentWindow();
    void Promise.all([win.outerPosition(), win.outerSize()]).then(([p, s]) => {
      this.prev ??= { x: p.x, y: p.y, w: s.width, h: s.height };
    });
  }

  private onGeometry(g: Geometry): void {
    const dpr = window.devicePixelRatio;
    const css = (r: Rect) => ({ w: r.w / dpr, h: r.h / dpr });
    const edges = this.prev && g.user && isTouched(this.store.get()) ? draggedEdges(this.prev, g) : null;
    if (this.prev && edges) {
      const win0 = css(this.prev);
      const win1 = css(g);
      this.store.set(
        anchorOnResize(this.store.get(), this.stage.fitScale(win0), win0, this.stage.fitScale(win1), win1, edges),
      );
    }
    this.prev = { x: g.x, y: g.y, w: g.w, h: g.h };
    this.stage.setWindowBox(css(g));
  }
}
