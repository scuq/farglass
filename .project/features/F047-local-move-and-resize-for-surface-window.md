---
id: F047
title: Local move and resize for surface windows
status: done
phase: P05
tags: [surface]
decisions: [ADR-007]
files: [web/src/apps/linux/Linux.tsx, web/src/wm/WindowFrame.tsx, web/src/surface/worker.ts, web/src/apps/types.ts, web/src/apps/AppHost.tsx, spikes/e2e/resize.mjs]
---

## Goal
A drag of a surface window is immediate at each network delay.

## Requirements
- The shell sends nothing while the user moves or changes the size of the window
- The window shows a snapshot of its last frame, at the top left, with its edge pixels in new area
- One size message at the end of the gesture
- Live content shows again after a frame at the new size, or after 500 ms
- If the app refuses the size, the window takes the size of the app

## Acceptance
A drag of a Linux app window sends no size to the app; the window shows the last frame with its right and bottom edges stretched; one size goes at the end; the frame takes the app size when the app refuses a size. resize.mjs passes, and surface, apps, popups, wm, and vitest (241) pass.
