import { createContext, useContext } from "react";
import type { MenuState } from "../components/ContextMenu";

export type Page = "home" | "list" | "search" | "history" | "library" | "settings";

export interface AppActions {
  navigate: (page: Page) => void;
  openAnime: (id: number, tab?: "details" | "entry" | "settings") => void;
  showMenu: (menu: MenuState) => void;
  notify: (message: string) => void;
}

export const AppContext = createContext<AppActions | null>(null);

export function useApp(): AppActions {
  const value = useContext(AppContext);
  if (!value) throw new Error("useApp outside AppContext");
  return value;
}
