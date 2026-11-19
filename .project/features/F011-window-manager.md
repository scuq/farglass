---
id: F011
title: Window manager
status: planned
phase: P03
tags: []
decisions: [ADR-001, ADR-012]
files: [web/src/wm/]
---

## Goal
Windows that move, change size, and stack at the refresh rate of the screen.

## Requirements
- Move with CSS transform and pointer capture
- Snap, tile layout, maximize, minimize
- Workspaces and Alt+Tab
- Unit tests for the layout logic

## Acceptance
