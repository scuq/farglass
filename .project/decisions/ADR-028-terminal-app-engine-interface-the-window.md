---
id: ADR-028
title: Terminal app: engine interface, the window id as terminal id, and acknowledgments after the parse
status: accepted
date: 2026-11-19
tags: [terminal, shell]
superseded_by:
files: [web/src/apps/terminal/, web/src/apps/types.ts, web/src/wm/controller.ts]
---

## Context
F015 needs a terminal window with xterm.js and WebGL behind a thin interface, copy and paste with OSC 52 and the Async Clipboard API, more than one terminal, and a delay of less than 5 ms from a key press to its echo. The agent keeps terminals by id (ADR-026).

## Decision
- TerminalEngine is the interface between the app and the emulator. xterm.ts implements it with xterm.js 6, the WebGL addon (with the DOM renderer as fallback), fit, unicode11, and clipboard. ghostty-web can replace it.
- The terminal id is the window id. A reload, a reconnect, or a second tab attaches to the same shell. Each window has its own shell.
- The app acknowledges output after xterm.js parsed it, in steps of 64 KiB or 20 ms after the last parse.
- AppContext gets onClose, onFocus, and isFocused. The terminal sends terminate only in onClose, thus only a real close of the window ends the shell. onFocus gives the keyboard focus to xterm.js after Alt+Tab or a task button.
- Copy and paste: Ctrl+Shift+C, Ctrl+Shift+V, Shift+Insert, and the native events of the browser. While the clipboard read of a paste is pending, the keyboard input waits, thus Enter after the paste comes after the pasted text. OSC 52 can set the clipboard of the browser. An OSC 52 read gives an empty text.
- EchoProbe writes one performance.measure farglass:terminal-echo for each key press, from the input to the parse of the next output. spikes/e2e/term.mjs fails at a 95th percentile of 5 ms or more.
- After an attach with a replay, the app resets the emulator before the scrollback. After an exit, Enter starts a new shell in the same window.

## Consequences
The terminal chunk is 479 kB (125 kB gzip) and loads only with the first terminal window. The browser keeps some keys of terminal programs (Ctrl+W, Ctrl+T, Ctrl+N) until Keyboard Lock (F017). A window in two tabs shows one shell, and the last resize of the two tabs sets its size. F016 adds links in the terminal.
