import { ipc } from "../ipc.ts";

let enabled = false;

/** Turn diagnostics on (the host decides: `PINIT_TRACE=1` in a debug build). */
export function enableDebug(on: boolean): void {
  enabled = on;
}

/** Prints in the `npm run dev:app` terminal (via the host) while diagnostics are on. */
export function dbg(msg: string): void {
  if (enabled) void ipc.viewerLog(msg).catch(() => {});
}

const r = (n: number) => Math.round(n * 10) / 10;

/** A DOM rect as "x,y WxH". */
export function fmtRect(e: Element | null): string {
  if (!e) return "none";
  const b = e.getBoundingClientRect();
  return `${r(b.left)},${r(b.top)} ${r(b.width)}x${r(b.height)}`;
}
