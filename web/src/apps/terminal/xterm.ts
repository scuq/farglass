/** The terminal engine with xterm.js and its WebGL renderer. */

import { ClipboardAddon, type IClipboardProvider } from "@xterm/addon-clipboard";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { isDarkTheme, isLcarsTheme, isReducedMotion, onThemeChange } from "../../appearance";
import type { TerminalEngine } from "./engine";
import { CopyGate } from "./osc52";
import { DEFAULT_TERMINAL, fontFamily, paletteOf, type TerminalSettings } from "./settings";

/**
 * Returns the clipboard for OSC 52. A program on the host can set the clipboard of the
 * browser, for example vim or tmux, but only shortly after a key or a click in this
 * terminal (F055). Otherwise `blocked` gets the text, and the app asks the user.
 * A program cannot read the clipboard: a read gives an empty text.
 */
function osc52(gate: CopyGate, blocked: (text: string) => void): IClipboardProvider {
  return {
    readText: () => "",
    writeText: async (_selection, text) => {
      if (gate.allows()) {
        await navigator.clipboard?.writeText(text);
      } else {
        blocked(text);
      }
    },
  };
}

const ANSI_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"] as const;

/** Returns the colors of the terminal: the color profile, or the theme of the shell (F107). */
function theme(settings: TerminalSettings) {
  const palette = paletteOf(settings.colors);
  if (palette) {
    const { ansi, ...base } = palette;
    return { ...base, cursorAccent: base.background, ...Object.fromEntries(ANSI_NAMES.map((name, index) => [name, ansi[index]])) };
  }
  if (isLcarsTheme()) {
    return { background: "#100b1a", foreground: "#f2eaf2", cursor: "#e6a446", cursorAccent: "#100b1a", selectionBackground: "#4a3566" };
  }
  return isDarkTheme()
    ? { background: "#1a1a19", foreground: "#ececea", cursor: "#ececea", selectionBackground: "#3b5c8f" }
    : { background: "#fbfbf9", foreground: "#1d1d1b", cursor: "#1d1d1b", selectionBackground: "#b5cbee" };
}

/**
 * Creates an xterm.js terminal in `element`.
 *
 * The WebGL renderer draws the terminal if the browser has WebGL2. If the
 * browser has none, or if it loses the WebGL context, the DOM renderer draws it.
 */
export function createXterm(element: HTMLElement, initial: TerminalSettings = DEFAULT_TERMINAL): TerminalEngine {
  let settings = initial;
  // The space around the rows has the background of the terminal.
  const paintBackground = (colors: { background?: string }) => element.style.setProperty("--term-bg", colors.background ?? "");
  const gate = new CopyGate();
  const copyHandlers: ((text: string) => void)[] = [];
  // Capture: the key or the click counts before xterm.js handles it.
  element.addEventListener("keydown", () => gate.input(), { capture: true });
  element.addEventListener("pointerdown", () => gate.input(), { capture: true });
  const terminal = new Terminal({
    allowProposedApi: true,
    // A blinking cursor is an effect: with reduced motion, it stays (F046).
    cursorBlink: !isReducedMotion(),
    fontFamily: fontFamily(settings.font),
    fontSize: settings.fontSize,
    scrollback: settings.scrollback,
    theme: theme(settings),
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.loadAddon(
    new ClipboardAddon(
      undefined,
      osc52(gate, (text) => {
        for (const handler of copyHandlers) handler(text);
      }),
    ),
  );
  const unicode = new Unicode11Addon();
  terminal.loadAddon(unicode);
  terminal.unicode.activeVersion = "11";
  terminal.open(element);
  paintBackground(terminal.options.theme ?? {});
  const stopTheme = onThemeChange(() => {
    terminal.options.theme = theme(settings);
    paintBackground(terminal.options.theme);
    terminal.options.cursorBlink = !isReducedMotion();
  });

  let renderer: TerminalEngine["renderer"] = "dom";
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => {
      webgl.dispose();
      renderer = "dom";
    });
    terminal.loadAddon(webgl);
    renderer = "webgl";
  } catch {
    // No WebGL2: the DOM renderer stays.
  }

  // While the clipboard read of a paste is pending, the input waits. Thus Enter after
  // Ctrl+Shift+V comes after the pasted text, also if the user types fast.
  let pasting: Promise<void> | null = null;
  const inputHandlers: ((data: string) => void)[] = [];
  const held: string[] = [];
  const deliver = (data: string) => {
    for (const handler of inputHandlers) {
      handler(data);
    }
  };
  terminal.onData((data) => {
    if (pasting) {
      held.push(data);
    } else {
      deliver(data);
    }
  });
  const pasteClipboard = () => {
    const read = navigator.clipboard?.readText() ?? Promise.resolve("");
    const done = (text: string) => {
      // The paste gives input itself, thus the hold ends first. The held keys follow the pasted text.
      pasting = null;
      if (text) {
        terminal.paste(text);
      }
      for (const data of held.splice(0)) {
        deliver(data);
      }
    };
    pasting = read.then(done, () => done(""));
  };

  // Copy and paste with the keys of a terminal: Ctrl+Shift+C, Ctrl+Shift+V, and Shift+Insert.
  terminal.attachCustomKeyEventHandler((event) => {
    if (event.type !== "keydown") {
      return true;
    }
    const key = event.key.toLowerCase();
    if (event.ctrlKey && event.shiftKey && !event.altKey && key === "c") {
      const text = terminal.getSelection();
      if (text) {
        void navigator.clipboard?.writeText(text);
      }
      event.preventDefault();
      return false;
    }
    if ((event.ctrlKey && event.shiftKey && !event.altKey && key === "v") || (event.shiftKey && event.key === "Insert")) {
      pasteClipboard();
      event.preventDefault();
      return false;
    }
    return true;
  });

  return {
    get renderer() {
      return renderer;
    },
    get cols() {
      return terminal.cols;
    },
    get rows() {
      return terminal.rows;
    },
    onBlockedCopy: (handler) => {
      copyHandlers.push(handler);
    },
    configure: (next) => {
      const font = next.font !== settings.font || next.fontSize !== settings.fontSize;
      settings = next;
      terminal.options.theme = theme(next);
      paintBackground(terminal.options.theme);
      terminal.options.scrollback = next.scrollback;
      if (font) {
        terminal.options.fontFamily = fontFamily(next.font);
        terminal.options.fontSize = next.fontSize;
        // A new cell size gives a new number of columns and rows.
        try {
          fit.fit();
        } catch {
          // The element has no size yet.
        }
      }
    },
    write: (data, done) => terminal.write(data, done),
    reset: () => terminal.reset(),
    focus: () => terminal.focus(),
    fit: () => {
      try {
        fit.fit();
      } catch {
        // The element has no size yet, for example in a minimized window.
      }
    },
    paste: (text) => terminal.paste(text),
    pasteClipboard,
    selectAll: () => terminal.selectAll(),
    clear: () => terminal.clear(),
    selection: () => terminal.getSelection(),
    text: () => {
      const buffer = terminal.buffer.active;
      const lines: string[] = [];
      for (let row = 0; row < buffer.length; row++) {
        lines.push(buffer.getLine(row)?.translateToString(true) ?? "");
      }
      return lines.join("\n").trimEnd();
    },
    onInput: (handler) => void inputHandlers.push(handler),
    onResize: (handler) => void terminal.onResize(({ cols, rows }) => handler(cols, rows)),
    onTitle: (handler) => void terminal.onTitleChange(handler),
    dispose: () => {
      stopTheme();
      terminal.dispose();
    },
  };
}
