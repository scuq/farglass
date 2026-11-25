/**
 * The thin interface between the terminal app and the terminal emulator.
 *
 * xterm.ts implements it with xterm.js. A different emulator, for example
 * ghostty-web, can implement it, and the app does not change.
 */

import type { TerminalSettings } from "./settings";

/** A terminal emulator in one element. */
export interface TerminalEngine {
  /** The renderer that draws the terminal. */
  readonly renderer: "webgl" | "dom";
  readonly cols: number;
  readonly rows: number;
  /** Applies the color profile, the font, and the scrollback (F107). */
  configure(settings: TerminalSettings): void;
  /** Writes output. `done` runs when the emulator parsed it. */
  write(data: Uint8Array | string, done?: () => void): void;
  /** Clears the screen, the scrollback, and the modes. */
  reset(): void;
  /** Gives the emulator the keyboard focus. */
  focus(): void;
  /** Changes the number of columns and rows to fill the element. */
  fit(): void;
  /** Sends text as a paste, with bracketed paste if the program asked for it. */
  paste(text: string): void;
  /** Pastes the clipboard of the browser. Keys that come during the read follow the pasted text. */
  pasteClipboard(): void;
  /** Selects the screen and the scrollback. */
  selectAll(): void;
  /** Removes the scrollback and the screen above the cursor line. */
  clear(): void;
  /** Returns the selected text. */
  selection(): string;
  /** Returns the text of the screen and the scrollback, one line for each row. */
  text(): string;
  /** Text that the user typed, as the program must get it. */
  onInput(handler: (data: string) => void): void;
  /** A new number of columns and rows. */
  onResize(handler: (cols: number, rows: number) => void): void;
  /** A program asked to write the clipboard (OSC 52) with no recent key or click of the user (F055). */
  onBlockedCopy(handler: (text: string) => void): void;
  /**
   * A key press before the emulator gets it. If `handler` returns true, the app used the
   * key: the emulator and the program do not get it.
   */
  onShortcut(handler: (event: KeyboardEvent) => boolean): void;
  /** A new title from the program (OSC 0 or OSC 2). */
  onTitle(handler: (title: string) => void): void;
  dispose(): void;
}
