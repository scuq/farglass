---
id: F077
title: LCARS endcap on the window title bars and segmented workspaces
status: done
phase:
tags: [web, ui]
decisions: [ADR-052]
files: [web/src/theme/lcars.css, web/index.html]
---

## Goal
The windows and the panel of the LCARS theme have an understated but clear LCARS identity. Input: a second external review.

## Requirements
- An endcap at the left of the title bar, under the icon: the accent color on the focused window, a muted lavender on other windows; the rest of the title bar stays dark
- The focused window has a clean 1px border in the accent color; other windows have a muted border; the corners match, with no double border or glow
- The title bar is 28px; the window buttons are neutral
- The workspaces are one segmented bar with rounded ends; the accent color fills the current workspace
- The login screen, the content of the windows, the settings layout, and the light and dark themes do not change; drag, resize, focus, and maximize work as before

## Acceptance
Chromium screenshots of farglassd in dev_user mode at 1280x800: each window title bar (28px) has an endcap at the left under its icon, in the accent color on the focused window and muted lavender on the others; the focused window has a 1px accent border, and a zoom of its bottom-right corner shows one clean rounded line also over the dark terminal; a maximized window has a square endcap at the top left. The workspaces are one segmented bar with rounded ends, and the accent color fills the current one. The five login screenshots, and the dark and light themes from a clean state, are byte-identical before and after. All web tests pass (263), and the e2e test wm (drag, snap, resize, Alt+Tab, reload) passes.
