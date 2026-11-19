---
id: ADR-030
title: One context menu for the shell, also in text fields, and no menu of the browser
status: accepted
date: 2026-11-19
tags: [shell, menu]
superseded_by:
files: [web/src/menu/, web/src/wm/menus.ts, web/src/apps/types.ts, web/src/App.tsx]
---

## Context
ADR-029 kept the menu of the browser in text fields and with Shift+right click. The user wants the feel of a native desktop: a right click always gives the menu of the shell. Text fields need cut, copy, paste, and select all. A context menu over the launcher must not close the launcher.

## Decision
- One ContextMenuHost at the root of the shell, thus also on the login screen: a popover in the top layer at the pointer, moved into the viewport. The arrow keys, Home, End, Enter, Space, Escape, and Tab work in it. It gives the focus back to the element of before.
- The shell blocks the menu of the browser everywhere, with and without Shift.
- setContextMenu(element, provider) registers a provider in a WeakMap. A right click asks the provider of the nearest element. A provider that gives no items blocks the menus around it: the content of each window has one, thus an app with no menu shows no menu.
- A text field (input of a text type, textarea, not the input of xterm.js) gets the text menu of the shell: cut, copy, paste, and select all, with the Async Clipboard API. A password field gives no cut and no copy. A read-only field gives no cut and no paste. Cut and paste change the field with setRangeText and send an input event.
- The menu opens with showPopover({ source }), with the clicked element as the source. Thus the menu is a child of a popover around the element, and a click in the menu does not close the launcher or the session menu. A browser with no source option shows the menu with no source.
- Shift+F10 and the ContextMenu key open the menu of the focused element with the first item selected. The key does not go to the element.
- windowMenu gives the menu of a window for its title bar, its edges, and its button in the panel. desktopMenu gives the menu of the empty desktop. AppContext.contextMenu lets an app register its menus. The terminal has copy, paste, select all, and clear.

## Consequences
No spelling suggestions of the browser in text fields. The paste needs the permission of the browser for the clipboard read, which Chromium asks for one time. Submenus are not available. F044 shows the shortcuts of the active keybinding scheme in the menus.
