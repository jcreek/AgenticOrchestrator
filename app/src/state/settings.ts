import { createContext, useContext } from "react";
import type { Settings_Serialize } from "../bindings";

export interface SettingsAccess {
  settings: Settings_Serialize;
  /** The terminal font to use: the one set, else an installed Nerd Font, else empty for the built-in fonts. */
  terminalFont: string;
  update: (changes: Partial<Settings_Serialize>) => void;
}

export const SettingsContext = createContext<SettingsAccess | null>(null);

export function useSettings(): SettingsAccess {
  const access = useContext(SettingsContext);
  if (!access) throw new Error("useSettings needs a SettingsContext provider");
  return access;
}
