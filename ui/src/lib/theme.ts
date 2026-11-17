import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import type { Settings } from "../api";

/**
 * Applies the color scheme setting: `data-theme` on the root element for the
 * page, and the native window theme for the title bar.
 */
export function useColorScheme(scheme: Settings["app.colorScheme"] | undefined) {
  useEffect(() => {
    if (!scheme) return;
    const root = document.documentElement;
    if (scheme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", scheme);
    getCurrentWindow()
      .setTheme(scheme === "system" ? null : scheme)
      .catch(() => {
        // Not available outside the app window; the page theme still applies.
      });
  }, [scheme]);
}
