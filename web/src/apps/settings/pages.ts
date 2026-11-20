/**
 * The pages of the settings app, in the order of the list. The launcher finds them by
 * their label and keywords (F042), thus this module does not load the app.
 */
export const PAGES = [
  { id: "appearance", label: "Appearance", keywords: ["theme", "dark", "light", "accent", "color", "colour", "text size", "font"] },
  { id: "keyboard", label: "Keyboard", keywords: ["shortcuts", "keys", "hotkeys", "keybindings", "tour", "mac", "windows"] },
  { id: "focus", label: "Focus", keywords: ["notifications", "do not disturb", "interrupt", "windows", "attention"] },
  { id: "datetime", label: "Date and time", keywords: ["clock", "time", "timezone", "time zone", "ntp", "date"] },
  { id: "system", label: "System", keywords: ["hostname", "host name", "computer name", "kernel", "operating system", "about"] },
  { id: "network", label: "Network", keywords: ["ip", "address", "ethernet", "wifi", "wi-fi", "wireless", "dns", "networkmanager"] },
  { id: "users", label: "Users and sessions", keywords: ["user", "session", "log out", "logout", "logind"] },
  { id: "power", label: "Power", keywords: ["restart", "reboot", "shut down", "shutdown", "power off"] },
] as const;

export type PageId = (typeof PAGES)[number]["id"];
