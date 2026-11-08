import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc } from "../ipc.ts";
import type { Stage } from "./Stage.ts";
import {
  defaultView, flipScreen, normalizeAngle, rotateAboutCenter, zoomAt,
} from "./transform.ts";
import type { ViewStore } from "./ViewStore.ts";

/** Wheel zoom, R+drag free rotation, Space/middle-drag pan, native window drag. */
export class PointerController {
  private panning = false;
  private rotating = false;
  private zooming = false;
  private zoomAnchor = { x: 0, y: 0 };
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
    // Space held while another window had focus never fires keydown here; ask the OS.
    el.addEventListener("pointerenter", () => void this.syncKeys());
    window.addEventListener("focus", () => void this.syncKeys());
    window.addEventListener("keydown", (e) => {
      if (e.code === "Space" && !this.spaceDown) {
        this.setSpace(true);
        e.preventDefault();
      } else if (e.code === "KeyR" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        this.rKeyDown = true;
        el.classList.add("rotate-mode");
      }
    });
    window.addEventListener("keyup", (e) => {
      if (e.code === "Space") {
        this.setSpace(false);
      } else if (e.code === "KeyR") {
        this.releaseRotate();
      }
    });
    window.addEventListener("blur", () => {
      this.setSpace(false);
      this.releaseRotate();
    });
  }

  private setSpace(on: boolean): void {
    this.spaceDown = on;
    this.stage.el.classList.toggle("pan-mode", on);
  }

  /** Refresh Space/Ctrl from the OS. Returns the physical Ctrl state. */
  private async syncKeys(): Promise<boolean> {
    try {
      const k = await ipc.heldKeys();
      this.setSpace(k.space);
      return k.ctrl;
    } catch {
      return false;
    }
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
    const factor = Math.exp(-e.deltaY / 400);
    const box = this.stage.windowBox();
    const c = { x: e.offsetX - box.w / 2, y: e.offsetY - box.h / 2 };
    this.store.update((v) => zoomAt(v, factor, c));
  }

  private onDown(e: PointerEvent): void {
    // A click can activate the window before this event, so hasFocus() alone doesn't mean
    // spaceDown is current: the sync from the focus handler may still be in flight.
    const mayBeStale = !document.hasFocus() || (!this.spaceDown && !this.rKeyDown);
    if (mayBeStale && e.button === 0) {
      // Resolve the OS key state before deciding what the drag means.
      e.preventDefault();
      const { pointerId, clientX, clientY, offsetX, offsetY } = e;
      void this.syncKeys().then((ctrl) => {
        this.begin(e.button, pointerId, clientX, clientY, offsetX, offsetY, ctrl || e.ctrlKey);
      });
      return;
    }
    this.begin(e.button, e.pointerId, e.clientX, e.clientY, e.offsetX, e.offsetY, e.ctrlKey, e);
  }

  private begin(
    button: number, pointerId: number, cx: number, cy: number, ox: number, oy: number,
    ctrl: boolean, e?: PointerEvent,
  ): void {
    if (this.rKeyDown && button === 0) {
      this.rotating = true;
      this.lastAngle = e ? this.angleAt(e) : this.angleFromClient(cx, cy);
      this.capture(pointerId);
      e?.preventDefault();
      return;
    }
    if (button === 0 && this.spaceDown && ctrl) {
      // Photoshop-style Ctrl+Space drag: right = zoom in, left = zoom out, around the press point.
      const box = this.stage.windowBox();
      this.zooming = true;
      this.zoomAnchor = { x: ox - box.w / 2, y: oy - box.h / 2 };
      this.last = { x: cx, y: cy };
      this.capture(pointerId);
      e?.preventDefault();
      return;
    }
    const pan = button === 1 || (button === 0 && this.spaceDown);
    if (pan) {
      this.panning = true;
      this.last = { x: cx, y: cy };
      this.capture(pointerId);
      e?.preventDefault();
    } else if (button === 0) {
      void getCurrentWindow().startDragging();
    }
  }

  private capture(pointerId: number): void {
    try {
      this.stage.el.setPointerCapture(pointerId);
    } catch {
      // Pointer already released while keys were being resolved.
      this.panning = this.rotating = this.zooming = false;
    }
  }

  private angleFromClient(x: number, y: number): number {
    const r = this.stage.el.getBoundingClientRect();
    return (Math.atan2(y - (r.top + r.height / 2), x - (r.left + r.width / 2)) * 180) / Math.PI;
  }

  private onMove(e: PointerEvent): void {
    if (this.zooming) {
      const dx = e.clientX - this.last.x;
      this.last = { x: e.clientX, y: e.clientY };
      if (dx !== 0) this.store.update((v) => zoomAt(v, Math.exp(dx / 200), this.zoomAnchor));
      return;
    }
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
    if (!this.panning && !this.rotating && !this.zooming) return;
    this.panning = false;
    this.rotating = false;
    this.zooming = false;
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
