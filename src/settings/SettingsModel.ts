import { ipc, type DisplayInfo, type Settings } from "../ipc.ts";

type Listener = () => void;

export function applyTheme(theme: Settings["theme"]): void {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

/** Settings state shared by all sections; saves are debounced and applied immediately in the UI. */
export class SettingsModel {
  displays: DisplayInfo[] = [];
  openCount = 0;
  private listeners: Listener[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;

  private constructor(public settings: Settings) {}

  static async load(): Promise<SettingsModel> {
    const m = new SettingsModel(await ipc.getSettings());
    await m.refresh();
    applyTheme(m.settings.theme);
    return m;
  }

  onChange(fn: Listener): void {
    this.listeners.push(fn);
  }

  private emit(): void {
    for (const l of this.listeners) l();
  }

  /** Re-read settings owned by the app (e.g. bridge status) without losing UI state. */
  async reloadSettings(): Promise<void> {
    this.settings = { ...this.settings, bridge_connected: (await ipc.getSettings()).bridge_connected };
    this.emit();
  }

  async refresh(): Promise<void> {
    [this.displays, this.openCount] = await Promise.all([ipc.listDisplays(), ipc.openCount()]);
    this.emit();
  }

  /** `rerender: false` for continuous inputs (sliders) so the control keeps focus/drag. */
  patch(partial: Partial<Settings>, rerender = true): void {
    this.settings = { ...this.settings, ...partial };
    if (partial.theme) applyTheme(partial.theme);
    if (rerender) this.emit();
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void ipc.setSettings(this.settings), 200);
  }
}
