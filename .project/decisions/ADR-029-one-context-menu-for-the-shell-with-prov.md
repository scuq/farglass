---
id: ADR-029
title: One context menu for the shell, with providers on elements
status: superseded
date: 2026-11-19
tags: [shell, menu]
superseded_by: ADR-030
files: [web/src/menu/, web/src/wm/menus.ts, web/src/apps/types.ts]
---

## Context
A right click showed the menu of the browser, which has no actions of the desktop. The window manager, the panel, the desktop, and each app need their own menus. Text fields need the menu of the browser for spelling and the native paste.

## Decision
- One ContextMenuHost on the desktop: a popover in the top layer at the pointer, moved into the viewport. The arrow keys, Home, End, Enter, Space, Escape, and Tab work in it. It gives the focus back to the element of before.
- setContextMenu(element, provider) registers a provider in a WeakMap. A right click asks the provider of the nearest element. A provider that gives no items blocks the menus around it: the content of each window has one, thus an app with no menu shows no menu.
- The shell blocks the menu of the browser everywhere on the desktop, except in text fields (not the input of xterm.js) and with Shift+right click.
- Shift+F10 and the ContextMenu key open the menu of the focused element with the first item selected. The key does not go to the element.
- windowMenu gives the menu of a window for its title bar, its edges, and its button in the panel. desktopMenu gives the menu of the empty desktop. AppContext.contextMenu lets an app register its menus. The terminal has copy, paste, select all, and clear.

## Consequences
A new app gets a context menu with one call. Submenus are not available: the window menu lists each other workspace as one item. F044 shows the shortcuts of the active keybinding scheme in the menus.
