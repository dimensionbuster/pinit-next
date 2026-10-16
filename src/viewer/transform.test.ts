import { test } from "node:test";
import assert from "node:assert/strict";
import {
  anchorOnResize, backdropCss, cssTransform, defaultView, fitScale, flipScreen, imageBox, normalizeAngle,
  rotateAboutCenter, rotateQuarter, zoomAt, MAX_ZOOM,
} from "./transform.ts";

const near = (a: number, b: number, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${a} !~ ${b}`);

test("odd quarter turns swap the element box", () => {
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 0), { w: 600, h: 300 });
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 1), { w: 300, h: 600 });
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 2), { w: 600, h: 300 });
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 3), { w: 300, h: 600 });
});

test("css transform order is flip, rotate, zoom, pan (right to left)", () => {
  const v = { ...defaultView(), zoom: 2, pan_x: 10, pan_y: -5, quarter_turns: 1, flip_h: true };
  assert.equal(
    cssTransform(v),
    "translate(10px, -5px) scale(2) rotate(90deg) scale(-1, 1)",
  );
});

test("zoomAt keeps the point under the cursor fixed and clamps", () => {
  const v = { ...defaultView(), zoom: 1, pan_x: 20, pan_y: 0 };
  const c = { x: 50, y: 30 };
  const z = zoomAt(v, 2, c);
  // image point under cursor before: (c - pan)/zoom ; after must be identical
  near((c.x - v.pan_x) / v.zoom, (c.x - z.pan_x) / z.zoom);
  near((c.y - v.pan_y) / v.zoom, (c.y - z.pan_y) / z.zoom);
  assert.equal(zoomAt({ ...v, zoom: MAX_ZOOM }, 5, c).zoom, MAX_ZOOM);
});

test("angles normalise into (-180, 180] and quarter turns wrap", () => {
  assert.equal(normalizeAngle(190), -170);
  assert.equal(normalizeAngle(-180), 180);
  assert.equal(normalizeAngle(725), 5);
  assert.equal(rotateQuarter(rotateQuarter(defaultView(), false), false).quarter_turns, 2);
  assert.equal(rotateQuarter({ ...defaultView(), quarter_turns: 3 }, true).quarter_turns, 0);
});

test("free rotation spins in place: zoom is never rescaled", () => {
  for (const a of [0, 10, 45, 90, -135]) {
    const css = cssTransform({ ...defaultView(), zoom: 1.5, fine_angle_deg: a });
    assert.match(css, /scale\(1\.5\)/, `angle ${a}`);
  }
});

/** Where a point of the unit-size image lands on screen (relative to window centre). */
function project(v: ReturnType<typeof defaultView>, p: { x: number; y: number }) {
  const fx = v.flip_h ? -1 : 1;
  const fy = v.flip_v ? -1 : 1;
  const a = ((v.quarter_turns * 90 + v.fine_angle_deg) * Math.PI) / 180;
  const x = p.x * fx;
  const y = p.y * fy;
  return {
    x: v.pan_x + v.zoom * (x * Math.cos(a) - y * Math.sin(a)),
    y: v.pan_y + v.zoom * (x * Math.sin(a) + y * Math.cos(a)),
  };
}

test("flipScreen mirrors the whole view about the window centre", () => {
  const v = { ...defaultView(), zoom: 1.7, pan_x: 40, pan_y: -25, quarter_turns: 1, fine_angle_deg: 30, flip_v: true };
  for (const p of [{ x: 10, y: 4 }, { x: -30, y: 22 }, { x: 0, y: 0 }]) {
    const before = project(v, p);
    const h = project(flipScreen(v, "h"), p);
    near(h.x, -before.x); near(h.y, before.y);
    const w = project(flipScreen(v, "v"), p);
    near(w.x, before.x); near(w.y, -before.y);
  }
  const twice = flipScreen(flipScreen(v, "h"), "h");
  near(twice.pan_x, v.pan_x);
  assert.equal(twice.quarter_turns, v.quarter_turns);
  near(twice.fine_angle_deg, v.fine_angle_deg);
  assert.equal(twice.flip_h, v.flip_h);
});

test("rotateAboutCenter turns the picture about the window centre", () => {
  const v = { ...defaultView(), zoom: 2, pan_x: 30, pan_y: 10, fine_angle_deg: 20 };
  const p = { x: 7, y: -3 };
  const b = project(v, p);
  const r = (35 * Math.PI) / 180;
  const a = project(rotateAboutCenter(v, 35), p);
  near(a.x, b.x * Math.cos(r) - b.y * Math.sin(r));
  near(a.y, b.x * Math.sin(r) + b.y * Math.cos(r));
});

test("anchorOnResize keeps on-screen scale and picture centre for every grip, rotated or not", () => {
  const nat = { w: 800, h: 600 };
  const views = [
    { ...defaultView(), zoom: 1.5, pan_x: 12, pan_y: -8 },
    { ...defaultView(), zoom: 0.7, pan_x: -40, pan_y: 25, quarter_turns: 1, fine_angle_deg: 30, flip_h: true },
  ];
  for (const v of views) {
    for (const west of [true, false]) {
      for (const north of [true, false]) {
        const from = { w: 400, h: 300 };
        const to = { w: 650, h: 220 };
        // window origin on screen; the edge opposite the dragged one stays put
        const o0 = { x: 1000, y: 500 };
        const o1 = { x: west ? o0.x + from.w - to.w : o0.x, y: north ? o0.y + from.h - to.h : o0.y };
        const fit0 = fitScale(from, v.quarter_turns, nat);
        const fit1 = fitScale(to, v.quarter_turns, nat);
        const out = anchorOnResize(v, from, to, fit0, fit1, { west, north });
        near(fit1 * out.zoom, fit0 * v.zoom);
        near(o1.x + to.w / 2 + out.pan_x, o0.x + from.w / 2 + v.pan_x);
        near(o1.y + to.h / 2 + out.pan_y, o0.y + from.h / 2 + v.pan_y);
      }
    }
  }
});

test("backdropCss builds rgba and repairs bad input", () => {
  assert.equal(backdropCss("#ffffff", 0.5), "rgba(255, 255, 255, 0.5)");
  assert.equal(backdropCss("#102030", 2), "rgba(16, 32, 48, 1)");
  assert.equal(backdropCss("javascript:x", NaN), "rgba(0, 0, 0, 0.4)");
});
