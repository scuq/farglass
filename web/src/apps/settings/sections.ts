/**
 * The cards of the settings pages, in the order of the pages. The filter of the settings
 * app finds a card by its title, its keywords, and the label of its page (F104).
 * This module has no DOM code.
 */

import { search } from "../../panel/search";
import { PAGES, type PageId } from "./pages";

interface SectionMeta {
  readonly id: string;
  readonly page: PageId;
  readonly title: string;
  /** The words that a user can type to find a setting of the card. */
  readonly keywords: readonly string[];
}

export const SECTIONS = [
  { id: "appearance-theme", page: "appearance", title: "Theme", keywords: ["dark", "light", "lcars", "color scheme", "colour scheme"] },
  { id: "appearance-accent", page: "appearance", title: "Accent color", keywords: ["colour", "blue", "green", "purple", "orange"] },
  { id: "appearance-text", page: "appearance", title: "Text size", keywords: ["font", "zoom", "scale", "large", "small"] },
  { id: "appearance-motion", page: "appearance", title: "Motion", keywords: ["animation", "reduced", "pulse", "blinking", "cursor"] },
  { id: "appearance-panel", page: "appearance", title: "Panel", keywords: ["bar", "dock", "top", "left", "taskbar"] },
  { id: "appearance-video", page: "appearance", title: "Videos in the host browser", keywords: ["redirect", "redirection", "youtube", "play", "smart"] },
  { id: "keyboard-scheme", page: "keyboard", title: "Shortcuts", keywords: ["keys", "hotkeys", "keybindings", "scheme", "mac", "windows", "linux", "hints", "tour"] },
  { id: "keyboard-layout", page: "keyboard", title: "Layout of the Linux apps", keywords: ["language", "us", "de", "at", "qwerty", "qwertz"] },
  { id: "keyboard-keys", page: "keyboard", title: "List of shortcuts", keywords: ["keys", "hotkeys", "keybindings", "action", "keyboard lock"] },
  { id: "terminal-colors", page: "terminal", title: "Color profile", keywords: ["colour", "palette", "theme", "ansi", "solarized", "gruvbox", "dracula", "nord", "tango"] },
  { id: "terminal-font", page: "terminal", title: "Font", keywords: ["font size", "typeface", "monospace", "fira", "jetbrains", "cascadia", "hack"] },
  { id: "terminal-scrollback", page: "terminal", title: "Scrollback", keywords: ["history", "buffer", "lines", "scroll"] },
  { id: "terminal-tabs", page: "terminal", title: "Tabs", keywords: ["tab bar", "new tab", "close", "exit", "shell stops"] },
  { id: "focus-rules", page: "focus", title: "Focus protection", keywords: ["notifications", "do not disturb", "interrupt", "windows", "attention", "quiet"] },
  { id: "datetime-clock", page: "datetime", title: "Clock in the panel", keywords: ["24-hour", "12-hour", "24 hours", "12 hours", "am", "pm", "crockford", "timestamp", "format"] },
  { id: "datetime-host", page: "datetime", title: "Time of the host", keywords: ["timezone", "time zone", "ntp", "network time", "date", "synchronized"] },
  { id: "system-names", page: "system", title: "Names", keywords: ["hostname", "host name", "computer name", "display name", "pretty"] },
  { id: "system-about", page: "system", title: "About this host", keywords: ["operating system", "kernel", "hardware", "chassis", "version", "vendor"] },
  { id: "network-devices", page: "network", title: "Devices", keywords: ["ip", "address", "ethernet", "wifi", "wi-fi", "wireless", "gateway", "networkmanager", "interface"] },
  { id: "passkeys-list", page: "passkeys", title: "Your passkeys", keywords: ["security key", "webauthn", "fido", "two-factor", "2fa", "second factor", "recovery code", "remove", "login"] },
  { id: "passkeys-add", page: "passkeys", title: "Add a passkey", keywords: ["security key", "webauthn", "fido", "two-factor", "2fa", "second factor", "phone", "new"] },
  { id: "users-sessions", page: "users", title: "Sessions", keywords: ["log out", "logout", "end", "logind", "seat", "locked"] },
  { id: "users-users", page: "users", title: "Users with a session or lingering processes", keywords: ["uid", "logind", "linger"] },
  { id: "resources-host", page: "resources", title: "This host", keywords: ["cpu", "memory", "ram", "network", "load", "usage", "graph", "history"] },
  { id: "resources-farglass", page: "resources", title: "Cost of the desktop", keywords: ["cpu", "memory", "ram", "bandwidth", "data rate", "performance", "usage", "score", "farglassd", "agent", "connection", "graph", "history"] },
  { id: "resources-apps", page: "resources", title: "Apps", keywords: ["cpu", "memory", "ram", "network", "windows", "terminal", "busy", "usage"] },
  { id: "power-actions", page: "power", title: "Restart and power off", keywords: ["reboot", "shut down", "shutdown", "stop"] },
] as const satisfies readonly SectionMeta[];

export type SectionId = (typeof SECTIONS)[number]["id"];

const PAGE_LABELS = new Map<PageId, string>(PAGES.map((page) => [page.id, page.label]));
const TITLES = new Map<SectionId, string>(SECTIONS.map((section) => [section.id, section.title]));
const ENTRIES = SECTIONS.map((section) => ({
  id: section.id as SectionId,
  title: section.title,
  keywords: [...section.keywords, PAGE_LABELS.get(section.page) ?? ""],
}));

/** Returns the title of a card. */
export function sectionTitle(id: SectionId): string {
  return TITLES.get(id) ?? id;
}

/**
 * Returns the cards that match `query`, or null if the query is empty.
 * An empty set tells that the filter is on and no card matches.
 * The rule is the one of the launcher: each word must be in the title, the id, or a keyword.
 */
export function matchSections(query: string): ReadonlySet<SectionId> | null {
  if (query.trim() === "") {
    return null;
  }
  return new Set(search(ENTRIES, query).map((entry) => entry.id));
}

/** Returns the pages that have a card in `matched`, in the order of the list of pages. */
export function pagesOf(matched: ReadonlySet<SectionId>): PageId[] {
  return PAGES.filter((page) => SECTIONS.some((section) => section.page === page.id && matched.has(section.id))).map((page) => page.id);
}
