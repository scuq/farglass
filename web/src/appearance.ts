/**
 * The appearance of the shell: the color theme, the accent color, and the text size.
 * The settings app saves it in the state store under {@link APPEARANCE_KEY},
 * thus each tab and each new login of the user gets it.
 */

/** The key in the state store. */
export const APPEARANCE_KEY = "appearance";

/** The color themes. "system" follows the setting of the client. */
export const THEMES = ["system", "light", "dark"] as const;
/** The accent colors. */
export const ACCENTS = ["blue", "green", "purple", "orange"] as const;
/** The text sizes in percent. */
export const TEXT_SIZES = [90, 100, 110, 125] as const;

export interface Appearance {
  readonly theme: (typeof THEMES)[number];
  readonly accent: (typeof ACCENTS)[number];
  readonly textSize: (typeof TEXT_SIZES)[number];
}

export const DEFAULT_APPEARANCE: Appearance = { theme: "system", accent: "blue", textSize: 100 };

const oneOf = <T>(list: readonly T[], value: unknown, fallback: T): T => (list.includes(value as T) ? (value as T) : fallback);

/** Returns a valid appearance from a value of the state store. Unknown fields get the default. */
export function parseAppearance(value: unknown): Appearance {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  return {
    theme: oneOf(THEMES, record.theme, DEFAULT_APPEARANCE.theme),
    accent: oneOf(ACCENTS, record.accent, DEFAULT_APPEARANCE.accent),
    textSize: oneOf(TEXT_SIZES, record.textSize, DEFAULT_APPEARANCE.textSize),
  };
}

/** Applies `appearance` to the document. The style sheet reads the data attributes of the root element. */
export function applyAppearance(appearance: Appearance, root: HTMLElement = document.documentElement): void {
  if (appearance.theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = appearance.theme;
  }
  root.dataset.accent = appearance.accent;
  root.style.fontSize = appearance.textSize === 100 ? "" : `${appearance.textSize}%`;
  document.dispatchEvent(new Event(THEME_EVENT));
}

const THEME_EVENT = "farglass-theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

/** Reports whether the shell shows the dark theme now: the choice of the user, or else the setting of the client. */
export function isDarkTheme(root: HTMLElement = document.documentElement): boolean {
  if (root.dataset.theme) {
    return root.dataset.theme === "dark";
  }
  return window.matchMedia?.(DARK_QUERY).matches ?? false;
}

/**
 * Calls `handler` when the theme can have changed: a new appearance, or a new
 * setting of the client. For code that draws its own colors, for example the terminal.
 * Returns the function that stops the calls.
 */
export function onThemeChange(handler: () => void): () => void {
  const media = window.matchMedia?.(DARK_QUERY);
  document.addEventListener(THEME_EVENT, handler);
  media?.addEventListener("change", handler);
  return () => {
    document.removeEventListener(THEME_EVENT, handler);
    media?.removeEventListener("change", handler);
  };
}
