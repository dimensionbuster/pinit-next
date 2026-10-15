import { test } from "node:test";
import assert from "node:assert/strict";
import {
  cssTransform, defaultView, fitScale, imageBox, normalizeAngle, rotateQuarter, zoomAt, MAX_ZOOM,
} from "./transform.ts";

const near = (a: number, b: number, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${a} !~ ${b}`);

test("odd quarter turns swap the element box", () => {
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 0), { w: 600, h: 300 });
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 1), { w: 300, h: 600 });
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 2), { w: 600, h: 300 });
  assert.deepEqual(imageBox({ w: 600, h: 300 }, 3), { w: 300, h: 600 });
});

test("fitScale: identity at 0/180, 1/sqrt2 at 45 on a square, shrinks wide boxes more", () => {
  near(fitScale({ w: 100, h: 100 }, 0), 1);
  near(fitScale({ w: 100, h: 100 }, 180), 1);
  near(fitScale({ w: 100, h: 100 }, 45), Math.SQRT1_2);
  assert.ok(fitScale({ w: 400, h: 100 }, 30) < fitScale({ w: 100, h: 100 }, 30));
});

test("rotated rectangle really fits inside the window after fitScale", () => {
  for (const [w, h] of [[300, 300], [600, 300], [300, 600]]) {
    for (let a = -180; a <= 180; a += 7) {
      const s = fitScale({ w, h }, a);
      const t = (a * Math.PI) / 180;
      const bw = s * (w * Math.abs(Math.cos(t)) + h * Math.abs(Math.sin(t)));
      const bh = s * (w * Math.abs(Math.sin(t)) + h * Math.abs(Math.cos(t)));
      assert.ok(bw <= w + 1e-6 && bh <= h + 1e-6, `${w}x${h} @${a}`);
    }
  }
});

test("css transform order is flip, rotate, zoom, pan (right to left)", () => {
  const v = { ...defaultView(), zoom: 2, pan_x: 10, pan_y: -5, quarter_turns: 1, flip_h: true };
  assert.equal(
    cssTransform(v, { w: 300, h: 300 }),
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
