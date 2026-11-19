---
id: F049
title: Context menus in the shell
status: done
phase: P03
tags: [shell, menu]
decisions: [ADR-024, ADR-030]
files: [web/src/menu/, web/src/wm/, web/src/panel/, web/src/apps/, spikes/e2e/term.mjs, README.md]
---

## Goal
A right click shows a menu of the shell, not the menu of the browser.

## Requirements
- One context menu for the shell: a popover at the pointer, with arrow keys, Enter, and Escape
- Shift+F10 and the ContextMenu key open the menu of the focused element
- The menu of the browser never shows. Text fields get the text menu of the shell: cut, copy, paste, select all
- Menus for the title bar and the window buttons of the panel: minimize, maximize, snap, move to a workspace, close
- A menu for the empty desktop: open each app, tile
- Apps add their own menus through AppContext. The terminal has copy, paste, select all, and clear

## Acceptance
- One context menu for the shell: a popover at the pointer, in the top layer. Chromium confirmed it on the title bar, in a terminal, on the empty desktop, and in the search field of the launcher. The arrow keys, Home, End, Enter, and Escape work in it. A menu at the edge of the viewport moves inside.
- Shift+F10 and the ContextMenu key open the menu of the focused element. A test confirms that the key does not reach the element, after Shift+F10 broke a command line in the terminal.
- The menu of the browser never shows, also with Shift. A text field gets cut, copy, paste, and select all. Chromium pasted into the search of the launcher from the text menu, and the launcher stayed open. Tests confirm no cut and no copy in a password field, and the input event of cut and paste.
- The title bar and the window button in the panel give minimize, maximize, snap, move to each other workspace, and close. The empty desktop gives each app and tile. An app with no menu shows no menu.
- AppContext.contextMenu lets apps add menus. The terminal menu copied all text to the clipboard and pasted the clipboard in Chromium.
