/**
 * The keyboard layout of the Linux apps (F094).
 *
 * The shell sends the physical key of each keyboard event to the compositor.
 * The compositor turns the key into a character with an XKB keymap, thus the
 * keymap must be the layout of the keyboard of the user. The shell suggests a
 * layout from the languages of the browser. The user can select a different one.
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

/** The layout when no language of the browser has one. */
export const FALLBACK_LAYOUT = "us";

/** The layouts of a language with a region, for example `de-CH`. A key with a region wins over a key with no region. */
const BY_LANGUAGE: Readonly<Record<string, string>> = {
  "en-gb": "gb",
  "en-ie": "ie",
  "en-ca": "us",
  "de-at": "at",
  "de-ch": "ch",
  "de-li": "ch",
  "fr-ch": "ch(fr)",
  "fr-be": "be",
  "nl-be": "be",
  "fr-ca": "ca",
  "pt-br": "br",
  "es-es": "es",
  sq: "al",
  ar: "ara",
  bg: "bg",
  cs: "cz",
  da: "dk",
  de: "de",
  el: "gr",
  en: "us",
  es: "latam",
  et: "ee",
  fi: "fi",
  fr: "fr",
  he: "il",
  hr: "hr",
  hu: "hu",
  is: "is",
  it: "it",
  ja: "jp",
  ko: "kr",
  lt: "lt",
  lv: "lv",
  mk: "mk",
  nb: "no",
  nl: "us",
  nn: "no",
  no: "no",
  pl: "pl",
  pt: "pt",
  ro: "ro",
  ru: "ru",
  sk: "sk",
  sl: "si",
  sr: "rs",
  sv: "se",
  th: "th",
  tr: "tr",
  uk: "ua",
};

/**
 * Returns the layout of the first language with a known layout, for example
 * `navigator.languages`. Spanish of Spain gets the Spanish layout, and other
 * Spanish gets the Latin American layout. Dutch of the Netherlands gets the US layout,
 * as most keyboards there have it.
 */
export function layoutFromLanguages(languages: readonly string[]): string {
  for (const language of languages) {
    const tag = language.toLowerCase().replace(/_/g, "-");
    const parts = tag.split("-");
    // A script subtag, for example sr-Latn-RS, is not a region.
    const region = parts.slice(1).find((part) => part.length === 2 || /^\d{3}$/.test(part));
    const layout = (region && BY_LANGUAGE[`${parts[0]}-${region}`]) || BY_LANGUAGE[parts[0] ?? ""];
    if (layout) {
      return layout;
    }
  }
  return FALLBACK_LAYOUT;
}

/** Returns the layout from the languages of this browser. */
export function detectLayout(): string {
  const languages = typeof navigator === "undefined" ? [] : navigator.languages?.length ? navigator.languages : [navigator.language];
  return layoutFromLanguages(languages.filter(Boolean));
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
