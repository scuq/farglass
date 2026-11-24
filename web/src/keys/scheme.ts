/**
 * The keybinding schemes of the shell (F044).
 *
 * A scheme follows the conventions that the user knows: "pc" for Windows and
 * Linux, "mac" for macOS. Each action has a primary binding, which can need
 * Keyboard Lock because the operating system of the client keeps it (Alt+Tab,
 * the Super key), and a second binding that reaches the browser on each client.
 * No binding takes a key of {@link RESERVED}: browser shortcuts, input methods,
 * screen readers, AltGr, characters, and keys that apps use.
 */

import { IS_MAC } from "../surface/keys";
import { isLayoutId } from "./layout";

/** An action that a key starts. */
export type ActionId =
  | "launcher"
  | "switch"
  | "switch-back"
  | "overview"
  | "layouts"
  | "tile"
  | "maximize"
  | "minimize"
  | "snap-left"
  | "snap-right"
  | "workspace-left"
  | "workspace-right"
  | "carry-left"
  | "carry-right"
  | "workspace-1"
  | "workspace-2"
  | "workspace-3"
  | "workspace-4"
  | "new-terminal"
  | "new-window"
  | "close-window";

/** One key with its modifiers. `code` is `KeyboardEvent.code`. */
export interface Chord {
  readonly code: string;
  readonly ctrl?: boolean;
  readonly alt?: boolean;
  readonly shift?: boolean;
  readonly meta?: boolean;
}

/** A chord of an action. */
export interface Binding {
  readonly chord: Chord;
  /** The chord reaches the shell only with Keyboard Lock: the client operating system or the browser keeps it. */
  readonly lock?: boolean;
  /** The chord acts only when a window of the shell has the focus: a terminal and a Linux app use it. */
  readonly shellOnly?: boolean;
}

export type SchemeId = "pc" | "mac";

/** The names of the actions, for the hints, the settings, and the tour. */
export const ACTION_LABELS: Readonly<Record<ActionId, string>> = {
  launcher: "Open or close the launcher",
  switch: "Switch to the next window",
  "switch-back": "Switch to the previous window",
  overview: "Open the overview",
  layouts: "Open the layout picker",
  tile: "Tile the windows in columns",
  maximize: "Maximize or restore",
  minimize: "Minimize",
  "snap-left": "Snap to the left half",
  "snap-right": "Snap to the right half",
  "workspace-left": "Show the workspace on the left",
  "workspace-right": "Show the workspace on the right",
  "carry-left": "Move the window to the workspace on the left",
  "carry-right": "Move the window to the workspace on the right",
  "workspace-1": "Show workspace 1",
  "workspace-2": "Show workspace 2",
  "workspace-3": "Show workspace 3",
  "workspace-4": "Show workspace 4",
  "new-terminal": "Open a new terminal",
  "new-window": "Open a new window of the app",
  "close-window": "Close the window",
};

const c = (code: string, mods: Omit<Chord, "code"> = {}): Chord => ({ code, ...mods });
const b = (chord: Chord, options: Omit<Binding, "chord"> = {}): Binding => ({ chord, ...options });

/** The pc scheme: Super with Keyboard Lock, and Alt+Shift with a letter on each client. */
const PC: Readonly<Record<ActionId, readonly Binding[]>> = {
  launcher: [b(c("MetaLeft", { meta: true }), { lock: true }), b(c("F1", { alt: true }))],
  switch: [b(c("Tab", { alt: true }), { lock: true }), b(c("Backquote", { alt: true }))],
  "switch-back": [b(c("Tab", { alt: true, shift: true }), { lock: true }), b(c("Backquote", { alt: true, shift: true }))],
  overview: [b(c("Tab", { meta: true }), { lock: true }), b(c("KeyG", { alt: true, shift: true }))],
  layouts: [b(c("KeyZ", { meta: true }), { lock: true }), b(c("KeyL", { alt: true, shift: true }))],
  tile: [b(c("KeyT", { meta: true }), { lock: true })],
  maximize: [b(c("ArrowUp", { meta: true }), { lock: true }), b(c("KeyM", { alt: true, shift: true }))],
  minimize: [b(c("ArrowDown", { meta: true }), { lock: true }), b(c("KeyH", { alt: true, shift: true }))],
  "snap-left": [b(c("ArrowLeft", { meta: true }), { lock: true }), b(c("BracketLeft", { alt: true, shift: true }))],
  "snap-right": [b(c("ArrowRight", { meta: true }), { lock: true }), b(c("BracketRight", { alt: true, shift: true }))],
  "workspace-left": [b(c("ArrowLeft", { ctrl: true, alt: true }))],
  "workspace-right": [b(c("ArrowRight", { ctrl: true, alt: true }))],
  "carry-left": [b(c("ArrowLeft", { ctrl: true, alt: true, shift: true }))],
  "carry-right": [b(c("ArrowRight", { ctrl: true, alt: true, shift: true }))],
  "workspace-1": [b(c("Digit1", { meta: true }), { lock: true }), b(c("Digit1", { alt: true, shift: true }))],
  "workspace-2": [b(c("Digit2", { meta: true }), { lock: true }), b(c("Digit2", { alt: true, shift: true }))],
  "workspace-3": [b(c("Digit3", { meta: true }), { lock: true }), b(c("Digit3", { alt: true, shift: true }))],
  "workspace-4": [b(c("Digit4", { meta: true }), { lock: true }), b(c("Digit4", { alt: true, shift: true }))],
  "new-terminal": [b(c("KeyT", { ctrl: true }), { lock: true, shellOnly: true }), b(c("KeyT", { alt: true, shift: true }))],
  "new-window": [b(c("KeyN", { ctrl: true }), { lock: true, shellOnly: true }), b(c("KeyN", { alt: true, shift: true }))],
  "close-window": [b(c("KeyW", { ctrl: true }), { lock: true, shellOnly: true }), b(c("KeyW", { alt: true, shift: true }))],
};

/**
 * The mac scheme: Ctrl+Shift with a letter, as Cmd and Option keys belong to macOS, the
 * browser, and the characters of Option. Option+Tab switches windows. With Keyboard Lock,
 * Cmd+T, Cmd+N, and Cmd+W act in the windows of the shell.
 */
const MAC: Readonly<Record<ActionId, readonly Binding[]>> = {
  launcher: [b(c("Space", { ctrl: true, shift: true }))],
  switch: [b(c("Tab", { alt: true }))],
  "switch-back": [b(c("Tab", { alt: true, shift: true }))],
  overview: [b(c("KeyG", { ctrl: true, shift: true }))],
  layouts: [b(c("KeyL", { ctrl: true, shift: true }))],
  tile: [],
  maximize: [b(c("KeyM", { ctrl: true, shift: true }))],
  minimize: [b(c("KeyH", { ctrl: true, shift: true }))],
  "snap-left": [b(c("BracketLeft", { ctrl: true, shift: true }))],
  "snap-right": [b(c("BracketRight", { ctrl: true, shift: true }))],
  "workspace-left": [b(c("ArrowLeft", { ctrl: true, shift: true }))],
  "workspace-right": [b(c("ArrowRight", { ctrl: true, shift: true }))],
  "carry-left": [b(c("Comma", { ctrl: true, shift: true }))],
  "carry-right": [b(c("Period", { ctrl: true, shift: true }))],
  "workspace-1": [b(c("Digit1", { ctrl: true, shift: true }))],
  "workspace-2": [b(c("Digit2", { ctrl: true, shift: true }))],
  "workspace-3": [b(c("Digit3", { ctrl: true, shift: true }))],
  "workspace-4": [b(c("Digit4", { ctrl: true, shift: true }))],
  "new-terminal": [b(c("KeyT", { meta: true }), { lock: true, shellOnly: true }), b(c("KeyT", { ctrl: true, shift: true }))],
  "new-window": [b(c("KeyN", { meta: true }), { lock: true, shellOnly: true }), b(c("KeyN", { ctrl: true, shift: true }))],
  "close-window": [b(c("KeyW", { meta: true }), { lock: true, shellOnly: true }), b(c("KeyW", { ctrl: true, shift: true }))],
};

export const SCHEMES: Readonly<Record<SchemeId, Readonly<Record<ActionId, readonly Binding[]>>>> = { pc: PC, mac: MAC };

const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS = "0123456789";
const all = (prefix: Omit<Chord, "code">, codes: readonly string[]) => codes.map((code) => c(code, prefix));
const letterCodes = [...LETTERS].map((l) => `Key${l}`);
const digitCodes = [...DIGITS].map((d) => `Digit${d}`);
const arrows = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];

/**
 * The chords that a scheme never takes, with the reason. A test checks each scheme against them.
 * A shellOnly chord with Keyboard Lock is a browser shortcut on purpose: it acts in shell windows only.
 */
export const RESERVED: Readonly<Record<SchemeId, readonly { readonly chord: Chord; readonly reason: string }[]>> = {
  pc: [
    ...["KeyL", "KeyD", "KeyH", "KeyJ", "KeyR", "KeyP", "KeyF", "KeyO", "KeyS", "KeyA", "KeyC", "KeyV", "KeyX", "KeyZ", "KeyY", "Tab"].map((code) => ({
      chord: c(code, { ctrl: true }),
      reason: "a browser or app shortcut",
    })),
    ...["KeyT", "KeyN", "KeyW", "Tab", "KeyI", "KeyJ", "KeyC", "KeyV", "KeyU", "KeyP"].map((code) => ({ chord: c(code, { ctrl: true, shift: true }), reason: "a browser shortcut, terminal copy and paste, or Unicode input" })),
    ...all({ ctrl: true, alt: true }, [...letterCodes, ...digitCodes, "Backquote", "BracketLeft", "BracketRight", "Backslash", "Minus", "Equal", "Comma", "Period"]).map((chord) => ({ chord, reason: "AltGr is Ctrl+Alt on Windows: it types characters" })),
    ...["ArrowUp", "ArrowDown"].map((code) => ({ chord: c(code, { ctrl: true, alt: true }), reason: "add a cursor in editors" })),
    ...all({ alt: true, shift: true }, [...arrows, "KeyF", "KeyO", "KeyA", "KeyI", "KeyD"]).map((chord) => ({ chord, reason: "an editor shortcut" })),
    ...all({ ctrl: true, shift: true }, arrows).map((chord) => ({ chord, reason: "select words or lines" })),
    ...all({ alt: true }, [...arrows, "Home", "F4", "Space"]).map((chord) => ({ chord, reason: "browser history, window menu, or close" })),
    ...["F5", "F11", "F12"].map((code) => ({ chord: c(code), reason: "a browser shortcut" })),
    { chord: c("Space", { ctrl: true }), reason: "an input method" },
    { chord: c("Space", { shift: true }), reason: "an input method" },
    { chord: c("Space", { meta: true }), reason: "an input method" },
    { chord: c("Period", { meta: true }), reason: "the emoji picker" },
    { chord: c("Period", { ctrl: true }), reason: "an input method" },
    { chord: c("Insert"), reason: "the key of a screen reader" },
    { chord: c("CapsLock"), reason: "the key of a screen reader" },
    { chord: c("KeyN", { ctrl: true, alt: true }), reason: "start NVDA" },
    { chord: c("KeyS", { meta: true, alt: true }), reason: "start Orca" },
  ],
  mac: [
    ...all({ meta: true }, [...letterCodes, ...digitCodes, "Tab", "Backquote", "Space", "Comma"]).filter((r) => !["KeyT", "KeyN", "KeyW"].includes(r.code)).map((chord) => ({ chord, reason: "macOS, the browser, or an app" })),
    ...all({ alt: true }, [...letterCodes, ...digitCodes, "Backquote", "BracketLeft", "BracketRight", "Comma", "Period", "Minus", "Equal"]).map((chord) => ({ chord, reason: "Option types a character" })),
    ...all({ alt: true, shift: true }, [...letterCodes, ...digitCodes, "Backquote"]).map((chord) => ({ chord, reason: "Option types a character" })),
    ...all({ ctrl: true, alt: true }, [...letterCodes, ...digitCodes, ...arrows, "Space", "Semicolon"]).map((chord) => ({ chord, reason: "VoiceOver" })),
    ...all({ ctrl: true }, [...arrows, "Space", ...letterCodes]).map((chord) => ({ chord, reason: "Mission Control, the input source, or the text keys of macOS" })),
    { chord: c("Space", { ctrl: true, meta: true }), reason: "the emoji picker" },
    { chord: c("Tab", { ctrl: true, shift: true }), reason: "the previous tab of the browser" },
    { chord: c("Tab", { ctrl: true }), reason: "the next tab of the browser" },
  ],
};

/** Reports whether two chords are the same keys. */
export function sameChord(a: Chord, b: Chord): boolean {
  return a.code === b.code && !!a.ctrl === !!b.ctrl && !!a.alt === !!b.alt && !!a.shift === !!b.shift && !!a.meta === !!b.meta;
}

/** Returns the scheme of the client operating system. */
export function detectScheme(): SchemeId {
  return IS_MAC ? "mac" : "pc";
}

type KeyEvent = Pick<KeyboardEvent, "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">;

/** Reports whether an event is the chord. A Super chord matches each of the two Super keys. */
function matches(event: KeyEvent, chord: Chord): boolean {
  const code = chord.code === "MetaLeft" && event.code === "MetaRight" ? "MetaLeft" : event.code;
  return code === chord.code && event.ctrlKey === !!chord.ctrl && event.altKey === !!chord.alt && event.shiftKey === !!chord.shift && event.metaKey === !!chord.meta;
}

/**
 * Returns the action of a key press in `scheme`, or null. `shellFocused` reports whether a window
 * of the shell has the focus: a shellOnly chord acts only then. The launcher chord of Super is a
 * tap, which the window manager detects on its own.
 */
export function actionOf(event: KeyEvent, scheme: SchemeId, shellFocused: boolean): ActionId | null {
  for (const [action, bindings] of Object.entries(SCHEMES[scheme]) as [ActionId, readonly Binding[]][]) {
    for (const binding of bindings) {
      if (binding.chord.code === "MetaLeft") continue;
      if (binding.shellOnly && !shellFocused) continue;
      if (matches(event, binding.chord)) return action;
    }
  }
  return null;
}

/** Returns the label of a chord in the convention of the scheme: "Alt+Shift+G", or "⌃⇧G" on a Mac. */
export function chordLabel(chord: Chord, scheme: SchemeId): string {
  const key = KEY_NAMES[chord.code] ?? chord.code.replace(/^Key|^Digit/, "");
  if (scheme === "mac") {
    return `${chord.ctrl ? "⌃" : ""}${chord.alt ? "⌥" : ""}${chord.shift ? "⇧" : ""}${chord.meta ? "⌘" : ""}${key}`;
  }
  if (chord.code === "MetaLeft") return "Super";
  return [chord.ctrl && "Ctrl", chord.alt && "Alt", chord.shift && "Shift", chord.meta && "Super", key].filter(Boolean).join("+");
}

const KEY_NAMES: Readonly<Record<string, string>> = {
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Backquote: "`",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Period: ".",
  Space: "Space",
  Tab: "Tab",
};

/**
 * Returns the hint of an action: the chord that works with no Keyboard Lock, else the first one.
 * Undefined if the scheme has no chord for it.
 */
export function hintOf(action: ActionId, scheme: SchemeId): string | undefined {
  const bindings = SCHEMES[scheme][action];
  const binding = bindings.find((each) => !each.lock) ?? bindings[0];
  return binding ? chordLabel(binding.chord, scheme) : undefined;
}

/** The modifiers of the second chords of a scheme: holding them shows the hints. */
export function hintModifiers(scheme: SchemeId): { ctrl: boolean; alt: boolean; shift: boolean } {
  return scheme === "mac" ? { ctrl: true, alt: false, shift: true } : { ctrl: false, alt: true, shift: true };
}

/** The keyboard settings in the state store. */
export const KEYBOARD_KEY = "keyboard";

export interface KeyboardSettings {
  /** The scheme, or "auto" for the scheme of the client operating system. */
  readonly scheme: "auto" | SchemeId;
  /** Holding the modifiers shows the hints. */
  readonly hints: boolean;
  /** The keyboard layout of the Linux apps (F094), or "auto" for the layout of the keyboard of the client (US if the browser cannot tell it). */
  readonly layout: "auto" | string;
  /** The layouts that the user selected last, the newest first, for the menu of the panel (F097). */
  readonly recent: readonly string[];
}

export const DEFAULT_KEYBOARD: KeyboardSettings = { scheme: "auto", hints: true, layout: "auto", recent: [] };

/** The number of layouts in the menu of the panel, apart from Automatic. */
export const RECENT_LAYOUTS = 4;

/** Returns the settings with the layout `layout`. A selected layout goes to the front of the recent layouts. */
export function withLayout(settings: KeyboardSettings, layout: "auto" | string): KeyboardSettings {
  const recent = layout === "auto" ? settings.recent : [layout, ...settings.recent.filter((each) => each !== layout)].slice(0, RECENT_LAYOUTS);
  return { ...settings, layout, recent };
}

/** Returns valid settings from a value of the state store. */
export function parseKeyboardSettings(value: unknown): KeyboardSettings {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  return {
    scheme: record.scheme === "pc" || record.scheme === "mac" ? record.scheme : "auto",
    hints: typeof record.hints === "boolean" ? record.hints : true,
    layout: isLayoutId(record.layout) ? record.layout : "auto",
    recent: Array.isArray(record.recent) ? [...new Set(record.recent.filter(isLayoutId))].slice(0, RECENT_LAYOUTS) : [],
  };
}

/** Returns the scheme of the settings. */
export function schemeOf(settings: KeyboardSettings): SchemeId {
  return settings.scheme === "auto" ? detectScheme() : settings.scheme;
}
