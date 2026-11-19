/** The terminal engine with xterm.js and its WebGL renderer. */

import { ClipboardAddon, type IClipboardProvider } from "@xterm/addon-clipboard";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { TerminalEngine } from "./engine";

/**
 * The clipboard for OSC 52. A program on the host can set the clipboard of the
 * browser, for example vim or tmux. A program cannot read it: a read gives an
 * empty text, because a program must not see the clipboard with no action of the user.
 */
const clipboard: IClipboardProvider = {
  readText: () => "",
  writeText: async (_selection, text) => {
    await navigator.clipboard?.writeText(text);
  },
};

/** Returns the colors of the terminal for the color scheme of the client. */
function theme() {
  const dark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  return dark
    ? { background: "#1a1a19", foreground: "#ececea", cursor: "#ececea", selectionBackground: "#3b5c8f" }
    : { background: "#fbfbf9", foreground: "#1d1d1b", cursor: "#1d1d1b", selectionBackground: "#b5cbee" };
}

/**
 * Creates an xterm.js terminal in `element`.
 *
 * The WebGL renderer draws the terminal if the browser has WebGL2. If the
 * browser has none, or if it loses the WebGL context, the DOM renderer draws it.
 */
export function createXterm(element: HTMLElement): TerminalEngine {
  const terminal = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    fontFamily: 'ui-monospace, "DejaVu Sans Mono", "Cascadia Mono", Menlo, monospace',
    fontSize: 14,
    scrollback: 10_000,
    theme: theme(),
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.loadAddon(new ClipboardAddon(undefined, clipboard));
  const unicode = new Unicode11Addon();
  terminal.loadAddon(unicode);
  terminal.unicode.activeVersion = "11";
  terminal.open(element);

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
    dispose: () => terminal.dispose(),
  };
}
