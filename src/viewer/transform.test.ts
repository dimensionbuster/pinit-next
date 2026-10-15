import { test } from "node:test";
import assert from "node:assert/strict";
import {
  backdropCss, cssTransform, defaultView, imageBox, normalizeAngle, rotateQuarter, zoomAt, MAX_ZOOM,
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

test("backdropCss builds rgba and repairs bad input", () => {
  assert.equal(backdropCss("#ffffff", 0.5), "rgba(255, 255, 255, 0.5)");
  assert.equal(backdropCss("#102030", 2), "rgba(16, 32, 48, 1)");
  assert.equal(backdropCss("javascript:x", NaN), "rgba(0, 0, 0, 0.4)");
});
