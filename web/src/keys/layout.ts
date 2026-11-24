/**
 * The keyboard layout of the Linux apps (F094).
 *
 * The shell sends the physical key of each keyboard event to the compositor.
 * The compositor turns the key into a character with an XKB keymap, thus the
 * keymap must be the layout of the keyboard of the user. In a browser with the
 * Keyboard API (Chromium), the shell finds the layout from the characters of the
 * keys. In a different browser, the layout is US until the user selects one.
 */

/** One XKB layout: `id` is the layout, or the layout and the variant as `layout(variant)`. */
export interface KeyboardLayout {
  readonly id: string;
  readonly label: string;
}

/** The layouts in the settings, by label. */
export const LAYOUTS: readonly KeyboardLayout[] = [
  { id: "al", label: "Albanian" },
  { id: "ara", label: "Arabic" },
  { id: "be", label: "Belgian" },
  { id: "br", label: "Brazilian Portuguese (ABNT2)" },
  { id: "bg", label: "Bulgarian" },
  { id: "ca", label: "Canadian French" },
  { id: "hr", label: "Croatian" },
  { id: "cz", label: "Czech" },
  { id: "dk", label: "Danish" },
  { id: "nl", label: "Dutch" },
  { id: "gb", label: "English (UK)" },
  { id: "us", label: "English (US)" },
  { id: "us(intl)", label: "English (US, international with dead keys)" },
  { id: "us(dvorak)", label: "English (Dvorak)" },
  { id: "us(colemak)", label: "English (Colemak)" },
  { id: "ee", label: "Estonian" },
  { id: "fi", label: "Finnish" },
  { id: "fr", label: "French" },
  { id: "de", label: "German" },
  { id: "at", label: "German (Austria)" },
  { id: "ch", label: "German (Switzerland)" },
  { id: "de(neo)", label: "German (Neo 2)" },
  { id: "gr", label: "Greek" },
  { id: "il", label: "Hebrew" },
  { id: "hu", label: "Hungarian" },
  { id: "is", label: "Icelandic" },
  { id: "ie", label: "Irish" },
  { id: "it", label: "Italian" },
  { id: "jp", label: "Japanese" },
  { id: "kr", label: "Korean" },
  { id: "latam", label: "Latin American Spanish" },
  { id: "lv", label: "Latvian" },
  { id: "lt", label: "Lithuanian" },
  { id: "mk", label: "Macedonian" },
  { id: "no", label: "Norwegian" },
  { id: "pl", label: "Polish" },
  { id: "pt", label: "Portuguese" },
  { id: "ro", label: "Romanian" },
  { id: "ru", label: "Russian" },
  { id: "rs", label: "Serbian" },
  { id: "sk", label: "Slovak" },
  { id: "si", label: "Slovenian" },
  { id: "es", label: "Spanish" },
  { id: "se", label: "Swedish" },
  { id: "ch(fr)", label: "Swiss French" },
  { id: "th", label: "Thai" },
  { id: "tr", label: "Turkish" },
  { id: "ua", label: "Ukrainian" },
];

/** The layout when the browser cannot tell the layout of the keyboard. */
export const FALLBACK_LAYOUT = "us";

/**
 * The characters of some keys of each layout, with no modifier: `KeyboardEvent.code`
 * to the character. The first layout whose keys all match wins, thus a layout comes
 * before a layout with fewer keys that it also matches. Dead keys are not in the
 * table: browsers do not give their character in the same way. Layouts with the
 * same keys, for example German and Austrian, give the first of them.
 */
const SIGNATURES: readonly { readonly id: string; readonly keys: Readonly<Record<string, string>> }[] = [
  { id: "ru", keys: { KeyQ: "й", KeyS: "ы" } },
  { id: "ua", keys: { KeyQ: "й", KeyS: "і" } },
  { id: "mk", keys: { KeyQ: "љ" } },
  { id: "gr", keys: { KeyA: "α" } },
  { id: "il", keys: { KeyA: "ש" } },
  { id: "ara", keys: { KeyA: "ش" } },
  { id: "th", keys: { KeyA: "ฟ" } },
  { id: "us(dvorak)", keys: { KeyD: "e", KeyS: "o" } },
  { id: "us(colemak)", keys: { KeyE: "f", KeyS: "r" } },
  { id: "de(neo)", keys: { KeyQ: "x", KeyW: "v" } },
  { id: "be", keys: { KeyQ: "a", Digit1: "&", Equal: "-" } },
  { id: "fr", keys: { KeyQ: "a", Digit1: "&" } },
  { id: "cz", keys: { KeyY: "z", Digit2: "ě", Digit3: "š" } },
  { id: "sk", keys: { KeyY: "z", Digit2: "ľ", Digit3: "š" } },
  { id: "hu", keys: { KeyY: "z", Digit0: "ö", Semicolon: "é" } },
  { id: "hr", keys: { KeyY: "z", Semicolon: "č", Quote: "ć" } },
  { id: "ch(fr)", keys: { KeyY: "z", Semicolon: "é", Quote: "à" } },
  { id: "ch", keys: { KeyY: "z", Semicolon: "ö", Backquote: "§" } },
  { id: "de", keys: { KeyY: "z", Semicolon: "ö", Quote: "ä" } },
  { id: "lt", keys: { Digit1: "ą", Digit2: "č" } },
  { id: "ro", keys: { Semicolon: "ș", Quote: "ț" } },
  { id: "tr", keys: { Semicolon: "ş", BracketLeft: "ğ" } },
  { id: "ee", keys: { Semicolon: "ö", BracketRight: "õ" } },
  { id: "se", keys: { Semicolon: "ö", Quote: "ä", BracketLeft: "å" } },
  { id: "no", keys: { Semicolon: "ø", Quote: "æ" } },
  { id: "dk", keys: { Semicolon: "æ", Quote: "ø" } },
  { id: "is", keys: { Semicolon: "æ", BracketLeft: "ð" } },
  { id: "es", keys: { Semicolon: "ñ", Equal: "¡" } },
  { id: "latam", keys: { Semicolon: "ñ", Equal: "¿" } },
  { id: "it", keys: { Semicolon: "ò", Quote: "à" } },
  { id: "pt", keys: { Semicolon: "ç", Equal: "«" } },
  { id: "br", keys: { Semicolon: "ç", BracketRight: "[" } },
  { id: "jp", keys: { Equal: "^", BracketLeft: "@" } },
  { id: "gb", keys: { KeyY: "y", Semicolon: ";", Quote: "'", Backslash: "#" } },
  { id: "us", keys: { KeyQ: "q", KeyY: "y", Semicolon: ";", Quote: "'", BracketLeft: "[", Backslash: "\\" } },
];

/** Returns the layout of a layout map of the Keyboard API, or null if no layout matches. */
export function layoutFromMap(map: ReadonlyMap<string, string>): string | null {
  const matches = (keys: Readonly<Record<string, string>>) => Object.entries(keys).every(([code, key]) => map.get(code)?.toLowerCase() === key);
  return SIGNATURES.find((signature) => matches(signature.keys))?.id ?? null;
}

/** The Keyboard API of Chromium, which other browsers do not have. */
interface KeyboardApi {
  getLayoutMap(): Promise<ReadonlyMap<string, string>>;
}

/**
 * Returns the layout of the keyboard of this computer, or null if the browser
 * cannot tell it. Ask again after the window gets the focus: the user can change
 * the layout of the operating system.
 */
export async function detectLayout(): Promise<string | null> {
  const keyboard = typeof navigator === "undefined" ? undefined : (navigator as Navigator & { keyboard?: Partial<KeyboardApi> }).keyboard;
  if (typeof keyboard?.getLayoutMap !== "function") {
    return null;
  }
  try {
    return layoutFromMap(await keyboard.getLayoutMap());
  } catch {
    // The browser refuses the call, for example in a frame of a different origin.
    return null;
  }
}

/** Returns the layout of the setting: the selected layout, or for "auto" the layout of the keyboard, or US. */
export async function resolveLayout(setting: "auto" | string): Promise<string> {
  return setting === "auto" ? ((await detectLayout()) ?? FALLBACK_LAYOUT) : setting;
}

/** A valid id: an XKB layout name, with an optional variant in parentheses. */
const ID = /^[a-z]{2,8}(\([a-z0-9_]{1,32}\))?$/;

/** Reports whether `id` is a valid layout id. */
export function isLayoutId(id: unknown): id is string {
  return typeof id === "string" && ID.test(id);
}

/** Returns the XKB layout and variant of a layout id: `ch(fr)` gives `ch` and `fr`. */
export function xkbOf(id: string): { layout: string; variant: string } {
  const match = /^([a-z]+)(?:\(([a-z0-9_]+)\))?$/.exec(id);
  return match ? { layout: match[1] ?? FALLBACK_LAYOUT, variant: match[2] ?? "" } : { layout: FALLBACK_LAYOUT, variant: "" };
}

/** Returns the label of a layout id, or the id if the list does not have it. */
export function layoutLabel(id: string): string {
  return LAYOUTS.find((layout) => layout.id === id)?.label ?? id;
}
