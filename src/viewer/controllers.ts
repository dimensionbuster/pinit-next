import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Stage } from "./Stage.ts";
import {
  defaultView, flipScreen, normalizeAngle, rotateAboutCenter, zoomAt,
} from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** Wheel zoom, Alt+wheel and R+drag free rotation, Space/middle-drag pan, native window drag. */
export class PointerController {
  private panning = false;
  private rotating = false;
  private spaceDown = false;
  private rKeyDown = false;
  private last = { x: 0, y: 0 };
  private lastAngle = 0;

  constructor(private readonly stage: Stage, private readonly store: ViewStore) {
    const el = stage.el;
    el.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    el.addEventListener("pointerdown", (e) => this.onDown(e));
    el.addEventListener("pointermove", (e) => this.onMove(e));
    el.addEventListener("pointerup", (e) => this.onUp(e));
    el.addEventListener("pointercancel", (e) => this.onUp(e));
    el.addEventListener("dblclick", () =>
      store.update((v) => ({ ...v, zoom: 1, pan_x: 0, pan_y: 0 })),
    );
    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" && !this.spaceDown) {
        this.spaceDown = true;
        el.classList.add("pan-mode");
        e.preventDefault();
      } else if (e.code === "KeyR" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        this.rKeyDown = true;
        el.classList.add("rotate-mode");
      }
    });
    window.addEventListener("keyup", (e) => {
      if (e.code === "Space") {
        this.spaceDown = false;
        el.classList.remove("pan-mode");
      } else if (e.code === "KeyR") {
        this.releaseRotate();
      }
    });
    window.addEventListener("blur", () => {
      this.spaceDown = false;
      el.classList.remove("pan-mode");
      this.releaseRotate();
    });
  }

  private releaseRotate(): void {
    this.rKeyDown = false;
    this.rotating = false;
    this.stage.el.classList.remove("rotate-mode");
  }

  /** Pointer angle (deg, clockwise) around the window centre. */
  private angleAt(e: PointerEvent): number {
    const r = this.stage.el.getBoundingClientRect();
    return (Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2)) * 180) / Math.PI;
  }

  private onWheel(e: WheelEvent): void {
    e.preventDefault();
    if (e.altKey) {
      const step = e.shiftKey ? 15 : 1;
      const dir = e.deltaY < 0 ? 1 : -1;
      this.store.update((v) => ({ ...v, fine_angle_deg: normalizeAngle(v.fine_angle_deg + dir * step) }));
      return;
    }
    const factor = Math.exp(-e.deltaY / 400);
    const box = this.stage.windowBox();
    const c = { x: e.offsetX - box.w / 2, y: e.offsetY - box.h / 2 };
    this.store.update((v) => zoomAt(v, factor, c));
  }

  private onDown(e: PointerEvent): void {
    if (this.rKeyDown && e.button === 0) {
      this.rotating = true;
      this.lastAngle = this.angleAt(e);
      this.stage.el.setPointerCapture(e.pointerId);
      e.preventDefault();
      return;
    }
    const pan = e.button === 1 || (e.button === 0 && this.spaceDown);
    if (pan) {
      this.panning = true;
      this.last = { x: e.clientX, y: e.clientY };
      this.stage.el.setPointerCapture(e.pointerId);
      e.preventDefault();
    } else if (e.button === 0) {
      void getCurrentWindow().startDragging();
    }
  }

  private onMove(e: PointerEvent): void {
    if (this.rotating) {
      const a = this.angleAt(e);
      const d = normalizeAngle(a - this.lastAngle);
      this.lastAngle = a;
      if (d !== 0) this.store.update((v) => rotateAboutCenter(v, d));
      return;
    }
    if (!this.panning) return;
    const dx = e.clientX - this.last.x;
    const dy = e.clientY - this.last.y;
    this.last = { x: e.clientX, y: e.clientY };
    this.store.update((v) => ({ ...v, pan_x: v.pan_x + dx, pan_y: v.pan_y + dy }));
  }

  private onUp(e: PointerEvent): void {
    if (!this.panning && !this.rotating) return;
    this.panning = false;
    this.rotating = false;
    this.stage.el.releasePointerCapture(e.pointerId);
  }
}

/** Keyboard shortcuts (see DESIGN.md). */
export class KeyboardController {
  constructor(store: ViewStore) {
    window.addEventListener("keydown", (e) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      switch (e.key) {
        case "h":
        case "H":
          store.update((v) => flipScreen(v, "h"));
          break;
        case "v":
        case "V":
          store.update((v) => flipScreen(v, "v"));
          break;
        case "0":
          store.set(defaultView());
          break;
        case "+":
        case "=":
          store.update((v) => zoomAt(v, 1.2, { x: 0, y: 0 }));
          break;
        case "-":
          store.update((v) => zoomAt(v, 1 / 1.2, { x: 0, y: 0 }));
          break;
      }
    });
  }
}
