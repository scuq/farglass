---
id: F103
title: System tray: context menus
status: planned
phase: P03
tags: [tray, menu]
decisions: [ADR-054]
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, crates/farglass-session/src/tray.rs, crates/farglass-session/src/link.rs, crates/farglass-session/tests/tray.rs, web/src/menu/, web/src/panel/Tray.tsx, web/src/panel/tray.ts, web/src/panel/tray.test.ts, spikes/e2e/tray.mjs, spikes/e2e/package.json, README.md, CHANGELOG.md]
---

## Goal
A right click on a tray icon opens the menu of the program, with its submenus and check marks.

## Requirements
- The agent reads the menu with AboutToShow and GetLayout of com.canonical.dbusmenu and sends it as a tree
- A click on an entry sends the clicked event to the program
- The shell menu shows check marks, radio marks, icons, and submenus, also with the keyboard
- An item with no menu gets SecondaryActivate on a right click; an item that is only a menu opens it on a left click
- A tab ignores a menu that it did not ask for
- Limits: a depth of 8 and 500 entries

## Acceptance
