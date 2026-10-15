// Single typed contract with the Rust side. Keep in sync with src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export interface ViewState {
  zoom: number;
  pan_x: number;
  pan_y: number;
  quarter_turns: number;
  fine_angle_deg: number;
  flip_h: boolean;
  flip_v: boolean;
}

export interface ViewerInit {
  path: string;
  width: number;
  height: number;
  view: ViewState;
  source_url: string | null;
}

export type Theme = "system" | "light" | "dark";
export type ModeKind = "grid" | "jigsaw";
export type Corner = "topleft" | "topright" | "bottomleft" | "bottomright";
export type Flow = "rows" | "columns";

export interface Settings {
  theme: Theme;
  corner: Corner;
  flow: Flow;
  mode: ModeKind;
  grid_line: number;
  jigsaw_base: number;
  jigsaw_min_scale: number;
  jigsaw_max_scale: number;
  preferred_display: number | null;
  show_in_taskbar: boolean;
  backdrop_color: string;
  backdrop_opacity: number;
  relayout_keep_sizes: boolean;
  bridge_connected: boolean;
}

export interface DisplayInfo {
  id: number;
  width: number;
  height: number;
  scale: number;
  primary: boolean;
}

export const ipc = {
  viewerInit: () => invoke<ViewerInit>("viewer_init"),
  viewerSetView: (view: ViewState) => invoke<void>("viewer_set_view", { view }),
  viewerFit: () => invoke<void>("viewer_fit"),
  viewerClose: () => invoke<void>("viewer_close"),
  viewerOpenSource: () => invoke<void>("viewer_open_source"),
  viewerSaveAs: () => invoke<boolean>("viewer_save_as"),

  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  listDisplays: () => invoke<DisplayInfo[]>("list_displays"),

  openCount: () => invoke<number>("open_count"),
  relayout: () => invoke<void>("relayout"),
  closeAll: () => invoke<void>("close_all"),
  pickAndOpen: () => invoke<number>("pick_and_open"),
  openUrlText: (text: string) => invoke<void>("open_url_text", { text }),
  openPaths: (paths: string[]) => invoke<number>("open_paths", { paths }),
  exportLayout: () => invoke<string | null>("export_layout"),
  importLayout: () => invoke<number | null>("import_layout"),
  revealExtension: () => invoke<void>("reveal_extension"),
  quit: () => invoke<void>("quit"),
};
