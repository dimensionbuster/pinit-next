import { h } from "../ui/dom.ts";
import { icon, type IconName } from "../ui/icons.ts";

export interface Option<T extends string> {
  value: T;
  label: string;
  icon?: IconName;
}

export function segmented<T extends string>(
  options: Option<T>[],
  current: T,
  onPick: (v: T) => void,
  label: string,
): HTMLElement {
  return h(
    "div",
    { class: "segmented", role: "group", "aria-label": label },
    ...options.map((o) =>
      h(
        "button",
        { type: "button", "aria-pressed": o.value === current, onClick: () => onPick(o.value) },
        o.icon && icon(o.icon),
        o.label,
      ),
    ),
  );
}

export function toggle(checked: boolean, label: string, onToggle: (v: boolean) => void): HTMLElement {
  return h("button", {
    class: "switch",
    role: "switch",
    type: "button",
    "aria-checked": checked,
    "aria-label": label,
    onClick: () => onToggle(!checked),
  });
}

export function field(title: string, hint: string | null, control: Node): HTMLElement {
  return h(
    "div",
    { class: "row between field" },
    h("div", {}, h("div", { class: "h2" }, title), hint && h("div", { class: "caption" }, hint)),
    control,
  );
}

export function card(title: string, ...body: Node[]): HTMLElement {
  return h("section", { class: "card stack" }, h("h2", { class: "h2 section-title" }, title), ...body);
}

export function toast(text: string): void {
  let host = document.querySelector(".toast-host");
  if (!host) {
    host = h("div", { class: "toast-host", role: "status" });
    document.body.append(host);
  }
  const t = h("div", { class: "toast" }, text);
  host.append(t);
  setTimeout(() => t.remove(), 4000);
}
