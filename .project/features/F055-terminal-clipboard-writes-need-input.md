---
id: F055
title: Terminal clipboard writes need input
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [web/src/apps/terminal/osc52.ts, web/src/apps/terminal/xterm.ts, web/src/apps/terminal/engine.ts, web/src/apps/terminal/Terminal.tsx, web/src/apps/terminal/terminal.css, spikes/e2e/term.mjs, README.md, docs/threats.md]
---

## Goal
Output of a program, for example of a remote host over ssh, cannot write the clipboard of the client computer without the user.

## Requirements
- An OSC 52 write goes to the clipboard only within a short time after a key or a click in that terminal, or after a confirm
- Review finding 8 in docs/threats.md

## Acceptance
Each terminal has a CopyGate: a keydown or pointerdown in its element opens a window of 3 s for OSC 52 writes. A later write does not touch the clipboard; the terminal shows a question with Copy and Ignore, and Copy writes the text with the click as the user action. A write in the replayed scrollback after a reload asks nothing. Tests: vitest osc52.test.ts; e2e term: a write right after a key passes, a write after sleep 4 asks, Ignore keeps the clipboard, Copy writes it; the reload step passes.
