// Pure view math. No DOM, so it can be unit-tested with `node --test`.
import type { ViewState } from "../ipc.ts";

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 10;

export interface Box {
  w: number;
  h: number;
}

export function defaultView(): ViewState {
  return { zoom: 1, pan_x: 0, pan_y: 0, quarter_turns: 0, fine_angle_deg: 0, flip_h: false, flip_v: false };
}

/** Element box before rotation: odd quarter turns swap the window's sides. */
export function imageBox(win: Box, quarterTurns: number): Box {
  return quarterTurns % 2 === 1 ? { w: win.h, h: win.w } : { w: win.w, h: win.h };
}

/** Scale that keeps a win-sized rectangle rotated by `angleDeg` inside the window. */
export function fitScale(win: Box, angleDeg: number): number {
  const t = (angleDeg * Math.PI) / 180;
  const c = Math.abs(Math.cos(t));
  const s = Math.abs(Math.sin(t));
  const sx = win.w / (win.w * c + win.h * s);
  const sy = win.h / (win.w * s + win.h * c);
  return Math.min(1, sx, sy);
}

/**
 * CSS transform for the image element (centred by layout).
 * Order matches core::view_state: flip -> rotate -> zoom -> pan.
 */
export function cssTransform(v: ViewState, win: Box): string {
  const fit = v.fine_angle_deg === 0 ? 1 : fitScale(win, v.fine_angle_deg);
  const angle = v.quarter_turns * 90 + v.fine_angle_deg;
  const fx = v.flip_h ? -1 : 1;
  const fy = v.flip_v ? -1 : 1;
  return `translate(${round(v.pan_x)}px, ${round(v.pan_y)}px) scale(${round(v.zoom * fit)}) rotate(${round(angle)}deg) scale(${fx}, ${fy})`;
}

const round = (n: number) => Math.round(n * 1000) / 1000;

export function clampZoom(z: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
}

/** Keep the point under the cursor fixed while zooming. `c` is cursor offset from the window centre. */
export function zoomAt(v: ViewState, factor: number, c: { x: number; y: number }): ViewState {
  const zoom = clampZoom(v.zoom * factor);
  const k = zoom / v.zoom;
  return { ...v, zoom, pan_x: c.x - (c.x - v.pan_x) * k, pan_y: c.y - (c.y - v.pan_y) * k };
}

/** Normalise into (-180, 180]. */
export function normalizeAngle(deg: number): number {
  let d = deg % 360;
  if (d > 180) d -= 360;
  else if (d <= -180) d += 360;
  return d;
}

export function rotateQuarter(v: ViewState, clockwise: boolean): ViewState {
  return { ...v, quarter_turns: (v.quarter_turns + (clockwise ? 1 : 3)) % 4 };
}
