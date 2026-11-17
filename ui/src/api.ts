// Typed wrappers around Tauri commands. Replace with tauri-specta generated
// bindings once the command surface grows (see MIGRATION_PLAN.md §4).
import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  dataDir: string;
  tables: string[];
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
};
