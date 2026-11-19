---
id: F015
title: Terminal app
status: done
phase: P03
tags: []
decisions: [ADR-012, ADR-026, ADR-027, ADR-028]
files: [web/src/apps/terminal/, web/src/apps/, web/src/wm/controller.ts, web/package.json, web/package-lock.json, crates/farglassd/src/http.rs, crates/farglassd/tests/server.rs, spikes/e2e/, README.md]
---

## Goal
A terminal window with a delay of less than 5 ms on loopback from key press to echo.

## Requirements
- xterm.js with the WebGL renderer behind a thin interface
- Copy and paste with OSC 52 and the Async Clipboard API
- Tabs or more than one window
- A speed test with the Performance API

## Acceptance
- The terminal app uses xterm.js 6 with the WebGL renderer behind the TerminalEngine interface. Chromium confirmed the WebGL renderer. With no WebGL2, the DOM renderer draws the terminal.
- Copy and paste: OSC 52 from a program set the clipboard of the browser, and Ctrl+Shift+V pasted it. Enter right after Ctrl+Shift+V comes after the pasted text. Ctrl+Shift+C and Shift+Insert also work. An OSC 52 read gives an empty text.
- More than one window: a second terminal window had its own shell. Closing a window ended its shell, and no scope or shell stayed after the tests.
- The speed test with the Performance API measured 41 key presses on loopback with optimized programs: median 0.7 ms to 0.9 ms, 95th percentile 1.3 ms to 1.9 ms in five runs, under the limit of 5 ms.
- A reload attached to the same shell with its scrollback and its variables. A job continued with no tab and its output was there at the next connection. A larger window gave more columns in stty size.
- No Content Security Policy violation, after ADR-027.
