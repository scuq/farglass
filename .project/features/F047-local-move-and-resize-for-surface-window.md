---
id: F047
title: Local move and resize for surface windows
status: planned
phase: P05
tags: [surface]
decisions: [ADR-007]
files: [web/src/surface/, web/src/wm/, crates/farglass-surface/]
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
