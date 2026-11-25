/**
 * The settings of the terminal app: the color profile, the font, and the scrollback (F107).
 * The settings app saves them in the state store under {@link TERMINAL_KEY}, thus each
 * terminal in each tab gets a change at once. This module has no DOM code and no xterm.js.
 */

/** The key in the state store. */
export const TERMINAL_KEY = "terminal";

/** The 16 ANSI colors and the base colors of a color profile. */
export interface Palette {
  readonly background: string;
  readonly foreground: string;
  readonly cursor: string;
  readonly selectionBackground: string;
  /** black, red, green, yellow, blue, magenta, cyan, white, then the 8 bright colors. */
  readonly ansi: readonly string[];
}

const SOLARIZED_ANSI = ["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5", "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"];

/** The color profiles. "theme" follows the theme of the desktop. */
export const PROFILES = {
  "solarized-dark": { label: "Solarized dark", palette: { background: "#002b36", foreground: "#839496", cursor: "#93a1a1", selectionBackground: "#073642", ansi: SOLARIZED_ANSI } },
  "solarized-light": { label: "Solarized light", palette: { background: "#fdf6e3", foreground: "#657b83", cursor: "#586e75", selectionBackground: "#eee8d5", ansi: SOLARIZED_ANSI } },
  "gruvbox-dark": {
    label: "Gruvbox dark",
    palette: { background: "#282828", foreground: "#ebdbb2", cursor: "#ebdbb2", selectionBackground: "#504945", ansi: ["#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984", "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2"] },
  },
  dracula: {
    label: "Dracula",
    palette: { background: "#282a36", foreground: "#f8f8f2", cursor: "#f8f8f2", selectionBackground: "#44475a", ansi: ["#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2", "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"] },
  },
  nord: {
    label: "Nord",
    palette: { background: "#2e3440", foreground: "#d8dee9", cursor: "#d8dee9", selectionBackground: "#434c5e", ansi: ["#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0", "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4"] },
  },
  "tango-light": {
    label: "Tango light",
    palette: { background: "#ffffff", foreground: "#2e3436", cursor: "#2e3436", selectionBackground: "#b5cbee", ansi: ["#2e3436", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf", "#555753", "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec"] },
  },
} as const satisfies Record<string, { label: string; palette: Palette }>;

export type ProfileId = "theme" | keyof typeof PROFILES;
export const PROFILE_IDS = ["theme", ...Object.keys(PROFILES)] as readonly ProfileId[];

/** The monospace stack after each font: a font that is not installed on the client falls back to it. */
export const MONOSPACE = 'ui-monospace, "DejaVu Sans Mono", "Cascadia Mono", Menlo, monospace';

/** The fonts. "default" is the monospace stack. The font must be installed on the client computer. */
export const FONTS = ["default", "DejaVu Sans Mono", "JetBrains Mono", "Fira Code", "Cascadia Code", "Source Code Pro", "Hack", "Ubuntu Mono", "Noto Sans Mono", "Menlo", "Consolas"] as const;
export const FONT_SIZES = [11, 12, 13, 14, 15, 16, 18, 20, 24] as const;
/** The number of lines that the terminal keeps above the screen. */
export const SCROLLBACKS = [1_000, 5_000, 10_000, 50_000, 100_000] as const;

/** When the tab bar shows (F108): with two or more tabs, or always. */
export const TAB_BARS = ["auto", "always"] as const;
/** What a tab does when its shell stops: it closes if the window has other tabs, or it stays with a message. */
export const ON_EXITS = ["close", "keep"] as const;
/** Where a new tab opens: after the active tab, or at the end. */
export const NEW_TABS = ["next", "end"] as const;

export interface TerminalSettings {
  readonly colors: ProfileId;
  readonly font: (typeof FONTS)[number];
  readonly fontSize: (typeof FONT_SIZES)[number];
  readonly scrollback: (typeof SCROLLBACKS)[number];
  readonly tabBar: (typeof TAB_BARS)[number];
  readonly onExit: (typeof ON_EXITS)[number];
  readonly newTab: (typeof NEW_TABS)[number];
}

export const DEFAULT_TERMINAL: TerminalSettings = { colors: "theme", font: "default", fontSize: 14, scrollback: 10_000, tabBar: "auto", onExit: "close", newTab: "end" };

const oneOf = <T>(list: readonly T[], value: unknown, fallback: T): T => (list.includes(value as T) ? (value as T) : fallback);

/** Returns valid settings from a value of the state store. Unknown fields get the default. */
export function parseTerminal(value: unknown): TerminalSettings {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  return {
    colors: oneOf(PROFILE_IDS, record.colors, DEFAULT_TERMINAL.colors),
    font: oneOf(FONTS, record.font, DEFAULT_TERMINAL.font),
    fontSize: oneOf(FONT_SIZES, record.fontSize, DEFAULT_TERMINAL.fontSize),
    scrollback: oneOf(SCROLLBACKS, record.scrollback, DEFAULT_TERMINAL.scrollback),
    tabBar: oneOf(TAB_BARS, record.tabBar, DEFAULT_TERMINAL.tabBar),
    onExit: oneOf(ON_EXITS, record.onExit, DEFAULT_TERMINAL.onExit),
    newTab: oneOf(NEW_TABS, record.newTab, DEFAULT_TERMINAL.newTab),
  };
}

/** Returns the CSS font family of a font. */
export function fontFamily(font: TerminalSettings["font"]): string {
  return font === "default" ? MONOSPACE : `"${font}", ${MONOSPACE}`;
}

/** Returns the palette of a profile, or undefined for "theme". */
export function paletteOf(colors: ProfileId): Palette | undefined {
  return colors === "theme" ? undefined : PROFILES[colors].palette;
}

/** Returns the label of a profile. */
export function profileLabel(colors: ProfileId): string {
  return colors === "theme" ? "Same as the desktop theme" : PROFILES[colors].label;
}
