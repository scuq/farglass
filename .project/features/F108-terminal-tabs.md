---
id: F108
title: Terminal tabs
status: done
phase:
tags: [terminal]
decisions: []
files: [web/src/apps/terminal/, web/src/apps/settings/, web/src/stats.ts, spikes/e2e/tabs.mjs, spikes/e2e/package.json, CHANGELOG.md]
---

## Goal
A terminal window holds several shells in tabs, which survive a reload.

## Requirements

## Acceptance
Each tab has its own agent terminal (first tab = window id, others <window>/<n>); layout in window state key terminal.tabs. + button, Alt+Shift+Enter, Alt+PageUp/PageDown, x, middle-click, exit closes a tab; last tab stays. Settings > Terminal > Tabs: tab bar, new tab position, on exit. Stats of tab terminals count for the window. spikes/e2e/tabs.mjs passes all 10 steps in Chromium (debug build), vitest 334 pass.
