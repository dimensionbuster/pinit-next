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

/**
 * Mirror the whole view about the window centre, in screen axes: the picture
 * you see is flipped left/right (`h`) or up/down (`v`) wherever it is panned or
 * rotated. M·Rot(a)·F = Rot(-a)·M·F, so the angle is negated and the flip toggled.
 */
export function flipScreen(v: ViewState, axis: "h" | "v"): ViewState {
  const q = (4 - v.quarter_turns) % 4;
  const fine = v.fine_angle_deg === 0 ? 0 : normalizeAngle(-v.fine_angle_deg);
  return axis === "h"
    ? { ...v, flip_h: !v.flip_h, quarter_turns: q, fine_angle_deg: fine, pan_x: -v.pan_x }
    : { ...v, flip_v: !v.flip_v, quarter_turns: q, fine_angle_deg: fine, pan_y: -v.pan_y };
}

/** Rotate the whole view by `deg` (clockwise) about the window centre. */
export function rotateAboutCenter(v: ViewState, deg: number): ViewState {
  const r = (deg * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return {
    ...v,
    fine_angle_deg: normalizeAngle(v.fine_angle_deg + deg),
    pan_x: v.pan_x * cos - v.pan_y * sin,
    pan_y: v.pan_x * sin + v.pan_y * cos,
  };
}

/** Scale at which the unzoomed image fits (object-fit: contain) a window of `win`. */
export function fitScale(win: Box, quarterTurns: number, natural: Box): number {
  if (natural.w <= 0 || natural.h <= 0) return 0;
  const box = imageBox(win, quarterTurns);
  return Math.min(box.w / natural.w, box.h / natural.h);
}

/** Which window edges a resize drag moves (the opposite edges stay put on screen). */
export interface DragEdges {
  west: boolean;
  north: boolean;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Which edges moved between two window rects. Per axis, if the size changed the edge that kept
 * its screen position is the fixed one; if neither did (app placed the window, DPI change) the
 * change is not an edge drag and null is returned.
 */
export function draggedEdges(a: Rect, b: Rect): DragEdges | null {
  if (a.w === b.w && a.h === b.h) return null;
  const axis = (p0: number, s0: number, p1: number, s1: number): boolean | null => {
    if (s0 === s1 || p0 === p1) return false; // unchanged, or the near edge stayed: far edge dragged
    return p0 + s0 === p1 + s1 ? true : null; // far edge stayed: near edge dragged
  };
  const west = axis(a.x, a.w, b.x, b.w);
  const north = axis(a.y, a.h, b.y, b.h);
  return west === null || north === null ? null : { west, north };
}

/**
 * Window resized from `win0` to `win1` by dragging `edges`: keep the picture fixed on screen so
 * only the mask edge moves. The edges not being dragged do not move, so the picture centre is
 * held at a constant distance from them; the centre (window centre + pan) shifts by half the
 * size change, towards the fixed side. On-screen scale is fit * zoom, so zoom absorbs the change
 * of the contain-fit scale. Needs no screen coordinates.
 */
export function anchorOnResize(
  v0: ViewState,
  fit0: number,
  win0: Box,
  fit1: number,
  win1: Box,
  edges: DragEdges,
): ViewState {
  return {
    ...v0,
    zoom: fit0 > 0 && fit1 > 0 ? (v0.zoom * fit0) / fit1 : v0.zoom,
    pan_x: v0.pan_x + ((win1.w - win0.w) / 2) * (edges.west ? 1 : -1),
    pan_y: v0.pan_y + ((win1.h - win0.h) / 2) * (edges.north ? 1 : -1),
  };
}

/** CSS colour for the viewer backdrop from a `#rrggbb` string and 0..1 opacity (falls back to black). */
export function backdropCss(color: string, opacity: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color);
  const [r, g, b] = m ? [m[1], m[2], m[3]].map((x) => parseInt(x, 16)) : [0, 0, 0];
  const a = Number.isFinite(opacity) ? Math.min(1, Math.max(0, opacity)) : 0.4;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}
