---
id: F043
title: Layout picker and overview with letter labels
status: done
phase: P03
tags: [shell]
decisions: [ADR-001]
files: [web/src/wm/geometry.ts, web/src/wm/model.ts, web/src/wm/LayoutPicker.tsx, web/src/wm/Overview.tsx, web/src/wm/controller.ts, web/src/wm/keys.ts, web/src/panel/Panel.tsx]
---

## Goal
The user selects a tile layout from a preview and jumps to a window with one key.

## Requirements
- The picker shows each layout with the open windows of the user: grid, center, wide, spotlight
- The overview shows all workspaces and gives each window a letter
- A letter key moves the focus to that window
- The overview is 2D and uses CSS transform only

## Acceptance
Tile opens a picker with a preview of each layout (columns, grid, center, wide, spotlight) drawn with the open windows of the workspace, chosen with the arrow keys or 1 to 5 and Enter; the overview shows all workspaces in a 2D grid placed and scaled with CSS transforms only, gives each window a letter (home row first), and the letter focuses that window also on another workspace. Keys: Super+Tab or Alt+Shift+G, Super+Z or Alt+Shift+L. Tests: layouts.test.tsx (no overlap and inside the area for each layout and count, letters, picker, overview) pass.
