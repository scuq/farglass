---
id: ADR-023
title: Window manager: pure state, transform-only gestures, and the desktop in the state store
status: accepted
date: 2026-11-19
tags: [shell, wm]
superseded_by:
files: [web/src/wm/, web/src/apps.tsx]
---

## Context
ADR-001 makes the client browser draw the shell, thus farglass needs its own window manager. A move must keep the refresh rate of the screen. The state store of the agent (ADR-022) keeps shell state. F012, F013, F043, F044, and F047 extend the window manager later.

## Decision
- The desktop is one immutable value (Desktop: windows in stack order, the order of use, the workspace). Each action is a pure function in model.ts. geometry.ts has the snap zones, the clamp, the size change, and the tile layout. Unit tests cover them with no DOM.
- Solid renders the windows with For keyed by the window id. The app of a window renders one time, untracked, thus a move, a snap, or a focus never starts the app again.
- A move or a size change uses pointer capture and writes only transform, width, and height to the element, one time for each animation frame. The desktop state changes one time, at the end of the gesture. Escape stops the gesture.
- A window keeps its floating rectangle. A snap zone (maximized, halves, quarters) shows on top of it, and a restore gives the rectangle back. A drag of a snapped window gives the floating size under the pointer.
- The desktop goes to the state store under the key desktop, 250 ms after the last change. All tabs of the user share it. A different screen size clamps the windows.
- An app gets its window id. It keeps its state under that id, and its element ids are unique.
- keys.ts is the one table of keys. Each action that needs Alt+Tab or Super has a second binding that reaches the browser with no Keyboard Lock: Alt+` and Ctrl+Alt+Arrow.
- The bar at the bottom (workspaces, tasks, new window, Tile) is temporary. The panel of F012 replaces it.

## Consequences
spikes/e2e/wm.mjs measures a drag in Chromium: the 95th percentile of the frame interval was 16.7 ms, with no long task. F013 moves the app table to modules. F043 adds more tile layouts to geometry.ts. F044 turns keys.ts into schemes. Each tab shows the same windows, thus an app window in two tabs shares its state key.
