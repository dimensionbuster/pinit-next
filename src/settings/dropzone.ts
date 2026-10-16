import { ipc } from "../ipc.ts";
import { h } from "../ui/dom.ts";
import { toast } from "./widgets.ts";

const IMAGE_NAME = /\.(png|jpe?g|gif|webp|bmp|jfif)$/i;

function readBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(r.error);
    r.onload = () => resolve(String(r.result).split(",", 2)[1] ?? "");
    r.readAsDataURL(file);
  });
}

/** First http(s) image address in a browser drag (`text/uri-list`, else an <img src> in `text/html`). */
function droppedUrl(dt: DataTransfer): string | null {
  const isHttp = (s: string) => /^https?:\/\//i.test(s);
  const list = dt.getData("text/uri-list").split(/\r?\n/).find((l) => l && !l.startsWith("#"));
  const src = new DOMParser().parseFromString(dt.getData("text/html"), "text/html").querySelector("img")?.src;
  if (src && isHttp(src)) return src; // the image itself beats the link wrapped around it
  return list && isHttp(list) ? list : null;
}

/** Drop images from Explorer or straight from a web page onto the settings window to pin them. */
export function installDropZone(): void {
  const overlay = h("div", { class: "drop-overlay", "aria-hidden": "true" }, h("div", { class: "drop-box" }, "여기에 놓으면 핀으로 고정돼요"));
  document.body.append(overlay);

  let depth = 0;
  const hasPayload = (e: DragEvent) => !!e.dataTransfer && ["Files", "text/uri-list", "text/html"].some((t) => e.dataTransfer!.types.includes(t));
  const show = (on: boolean) => overlay.classList.toggle("on", on);

  window.addEventListener("dragenter", (e) => {
    if (!hasPayload(e)) return;
    e.preventDefault();
    depth++;
    show(true);
  });
  window.addEventListener("dragover", (e) => {
    if (!hasPayload(e)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  });
  window.addEventListener("dragleave", () => {
    depth = Math.max(0, depth - 1);
    if (depth === 0) show(false);
  });
  window.addEventListener("drop", (e) => {
    if (!hasPayload(e) || !e.dataTransfer) return;
    e.preventDefault();
    depth = 0;
    show(false);
    void handleDrop(e.dataTransfer);
  });
}

async function handleDrop(dt: DataTransfer): Promise<void> {
  // Read everything first: DataTransfer is only valid synchronously during the event.
  const files = [...dt.files].filter((f) => f.type.startsWith("image/") || IMAGE_NAME.test(f.name));
  const url = files.length ? null : droppedUrl(dt);
  try {
    if (files.length) {
      for (const f of files) await ipc.openDropped(await readBase64(f), f.type || "image/png");
    } else if (url) {
      await ipc.openUrlText(url);
    } else if ([...dt.files].some((f) => f.type.startsWith("video/"))) {
      toast("영상은 탐색기에서 파일을 우클릭 → 'PinIt에 고정'으로 열어 주세요");
    } else {
      toast("이미지를 찾지 못했어요");
    }
  } catch (err) {
    toast(String(err));
  }
}
