---
id: F011
title: Window manager
status: done
phase: P03
tags: []
decisions: [ADR-001, ADR-012, ADR-022, ADR-023]
files: [web/src/wm/, web/src/apps.tsx, web/src/Echo.tsx, web/src/App.tsx, web/src/shell.css, spikes/e2e/wm.mjs, spikes/e2e/echo.mjs]
---

## Goal
Windows that move, change size, and stack at the refresh rate of the screen.

## Requirements
- Move with CSS transform and pointer capture
- Snap, tile layout, maximize, minimize
- Workspaces and Alt+Tab
- Unit tests for the layout logic

## Acceptance
- A window moves with CSS transform and pointer capture, one write for each animation frame. Chromium measured a drag of 90 steps: frame interval median and 95th percentile 16.7 ms, a new transform in each step, and no long task.
- A drag to an edge snaps the window: top maximized, left or right half, a corner a quarter, with a preview. The title bar buttons and Super+Up and Super+Down maximize, restore, and minimize. Tile places the windows of the workspace in columns with no overlap. A drag of an edge or a corner changes the size.
- Four workspaces with buttons and Ctrl+Alt+Arrow. Alt+Tab and Alt+` open the switcher in the order of use. Chromium confirmed Alt+Tab.
- 37 unit tests cover the geometry, the model, the switcher, and the keys. 9 DOM tests cover the desktop. One test confirms that a change of a window does not start its app again.
- The desktop comes back after a reload from the state store, with the snap and the size.
