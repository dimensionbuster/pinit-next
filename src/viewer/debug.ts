import { ipc } from "../ipc.ts";

/** Prints in the `npm run dev:app` terminal (via the host). No-op in release builds. */
export function dbg(msg: string): void {
  if (import.meta.env.DEV) void ipc.viewerLog(msg).catch(() => {});
}

const r = (n: number) => Math.round(n * 10) / 10;

/** A DOM rect as "x,y WxH". */
export function fmtRect(e: Element | null): string {
  if (!e) return "none";
  const b = e.getBoundingClientRect();
  return `${r(b.left)},${r(b.top)} ${r(b.width)}x${r(b.height)}`;
}
