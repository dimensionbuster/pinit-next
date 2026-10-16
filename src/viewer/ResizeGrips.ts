import { getCurrentWindow } from "@tauri-apps/api/window";
import { h } from "../ui/dom.ts";

type Dir = "North" | "South" | "East" | "West" | "NorthEast" | "NorthWest" | "SouthEast" | "SouthWest";

const GRIPS: Dir[] = ["North", "South", "East", "West", "NorthEast", "NorthWest", "SouthEast", "SouthWest"];

/** Frameless windows have no native borders: invisible edge/corner grips start OS resize drags. */
export function createResizeGrips(): HTMLElement[] {
  return GRIPS.map((dir) =>
    h("div", {
      class: `grip ${dir.toLowerCase()}`,
      onPointerDown: (e) => {
        if ((e as PointerEvent).button !== 0) return;
        e.preventDefault();
        void getCurrentWindow().startResizeDragging(dir);
      },
    }),
  );
}
