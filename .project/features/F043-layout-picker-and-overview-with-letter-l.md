---
id: F043
title: Layout picker and overview with letter labels
status: planned
phase: P03
tags: [shell]
decisions: [ADR-001]
files: [web/src/wm/]
---

## Goal
The user selects a tile layout from a preview and jumps to a window with one key.

## Requirements
- The picker shows each layout with the open windows of the user: grid, center, wide, spotlight
- The overview shows all workspaces and gives each window a letter
- A letter key moves the focus to that window
- The overview is 2D and uses CSS transform only

## Acceptance
