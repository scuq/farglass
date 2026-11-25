/**
 * The pages of the settings app, in the order of the list. The launcher finds them by
 * their label and keywords (F042), thus this module does not load the app.
 */
export const PAGES = [
  { id: "appearance", label: "Appearance", keywords: ["theme", "dark", "light", "accent", "color", "colour", "text size", "font", "motion", "animation", "video", "redirect", "youtube"] },
  { id: "keyboard", label: "Keyboard", keywords: ["shortcuts", "keys", "hotkeys", "keybindings", "tour", "mac", "windows"] },
  { id: "terminal", label: "Terminal", keywords: ["shell", "console", "color profile", "colour", "palette", "font", "font size", "scrollback", "history", "lines", "solarized", "gruvbox", "dracula", "nord", "tabs", "tab bar", "split", "panes"] },
  { id: "focus", label: "Focus", keywords: ["notifications", "do not disturb", "interrupt", "windows", "attention"] },
  { id: "datetime", label: "Date and time", keywords: ["clock", "time", "timezone", "time zone", "ntp", "date", "24-hour", "12-hour", "am", "pm", "crockford", "timestamp"] },
  { id: "system", label: "System", keywords: ["hostname", "host name", "computer name", "kernel", "operating system", "about"] },
  { id: "network", label: "Network", keywords: ["ip", "address", "ethernet", "wifi", "wi-fi", "wireless", "dns", "networkmanager"] },
  { id: "passkeys", label: "Passkeys", keywords: ["passkey", "security key", "webauthn", "fido", "two-factor", "2fa", "second factor", "recovery code", "login", "password"] },
  { id: "users", label: "Users and sessions", keywords: ["user", "session", "log out", "logout", "logind"] },
  { id: "resources", label: "Resources", keywords: ["cpu", "memory", "ram", "bandwidth", "data rate", "performance", "usage", "score", "cost"] },
  { id: "power", label: "Power", keywords: ["restart", "reboot", "shut down", "shutdown", "power off"] },
] as const;

export type PageId = (typeof PAGES)[number]["id"];
