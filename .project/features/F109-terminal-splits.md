---
id: F109
title: Terminal splits
status: done
phase:
tags: [terminal]
decisions: []
files: [web/src/apps/terminal/, web/src/apps/settings/, spikes/e2e/tabs.mjs, CHANGELOG.md]
---

## Goal
A terminal tab holds several shells in split panes that the user can resize, and they survive a reload.

## Requirements

## Acceptance
Each tab has a split tree (row/column, ratio 0.1-0.9) of panes, each with its own agent terminal. All panes are rendered flat at computed boxes, so a split, close, or tab switch never remounts a terminal. Alt+Shift+\\ and Alt+Shift+- split, Alt+Shift+arrows move the focus, dividers drag and double-click to reset, exit closes a pane. Settings: dim inactive panes, focus follows mouse. Old F108 layouts parse. spikes/e2e/tabs.mjs: 19/19 pass in Chromium; vitest 340 pass.
