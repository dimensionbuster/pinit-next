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

/**
 * CSS transform for the image element (centred by layout).
 * Order matches core::view_state: flip -> rotate -> zoom -> pan.
 * Free rotation never rescales the image: it spins in place at the current zoom.
 */
export function cssTransform(v: ViewState): string {
  const angle = v.quarter_turns * 90 + v.fine_angle_deg;
  const fx = v.flip_h ? -1 : 1;
  const fy = v.flip_v ? -1 : 1;
  return `translate(${round(v.pan_x)}px, ${round(v.pan_y)}px) scale(${round(v.zoom)}) rotate(${round(angle)}deg) scale(${fx}, ${fy})`;
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

/** CSS colour for the viewer backdrop from a `#rrggbb` string and 0..1 opacity (falls back to black). */
export function backdropCss(color: string, opacity: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color);
  const [r, g, b] = m ? [m[1], m[2], m[3]].map((x) => parseInt(x, 16)) : [0, 0, 0];
  const a = Number.isFinite(opacity) ? Math.min(1, Math.max(0, opacity)) : 0.4;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}
