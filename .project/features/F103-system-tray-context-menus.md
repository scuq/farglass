---
id: F103
title: System tray: context menus
status: done
phase: P03
tags: [tray, menu]
decisions: [ADR-054]
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, crates/farglass-session/src/tray.rs, crates/farglass-session/tests/tray.rs, crates/farglass-session/examples/tray-item.rs, web/src/menu/, web/src/panel/Tray.tsx, web/src/panel/tray.ts, web/src/panel/tray.test.ts, spikes/e2e/tray.mjs, spikes/e2e/package.json, README.md, CHANGELOG.md]
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
A right click on a tray icon, Shift+F10, or the ContextMenu key opens the menu of the program below the icon, with check marks, radio marks, icons, and submenus. A click on an entry sends the event clicked to the program. An item with no menu gets SecondaryActivate, an item that is only a menu opens it on a click, and an item that refuses Activate is a menu from then on. A tab ignores a menu that it did not ask for or that comes after 3 s. The agent reads the menu at each request with AboutToShow and GetLayout, with a depth of 8 and 500 entries at most, and does not follow LayoutUpdated. Tests: tray.rs unit tests check the tree, the limits, and the mnemonic marks. tests/tray.rs reads the menu of a fake program on the session bus, sends a click, and opens the menu of an item that refuses Activate. menu.test.tsx checks the marks, the submenu with pointer and keys, and the requests of a tray button. tray.mjs checks the icon, a click, the menu, the submenu, Quit, and Shift+F10 in Chromium with the example tray-item. A real libayatana indicator gave its icon and its menu to the agent in a manual run. Not tested: the menu of Bitwarden and of Seafile.
