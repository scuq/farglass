---
id: F075
title: LCARS theme: neutral surfaces, color only for state and identity
status: done
phase:
tags: [web, ui]
decisions: [ADR-049, ADR-051]
files: [web/src/theme/lcars.css, web/index.html, web/src/apps/terminal/xterm.ts]
---

## Goal
The LCARS theme is calm enough for a day of work, and the LCARS look stays clear. Input: an external review of F071.

## Requirements
- Neutral surfaces; a color has one role (ADR-051)
- Panel: the apps elbow and the session cap stay; a workspace or window button is neutral, and a line in the accent color marks the pressed one
- Window: a dark title bar; the border and a cap in the accent color mark the focus
- Launcher and menus: the selection is a tint with a marker, not a fill
- Settings: neutral rows in the list of pages, lavender for the open page; a group has a divider and no box
- Buttons: the main action is a pill in the accent color, other buttons are neutral; inputs are flat
- The login screen and the light and dark themes do not change
- No change of markup, behavior, or dependencies

## Acceptance
Chromium screenshots of farglassd in dev_user mode at 1280x800, before and after: the panel keeps the apps elbow and the session cap, and its other buttons are neutral with a line in the accent color on the pressed one; a window has a dark title bar, and the border and a cap mark the focus; the launcher selection is a tint with a marker; the settings app has neutral rows and groups with a divider. Five screenshots of the login screen (start, focused field, focused button, second step, failed login) are byte-identical before and after. The dark and the light theme, from a clean state with the launcher open, are byte-identical too. Contrast by calculation: secondary text 6.1:1 or more, borders of controls 3.2:1 or more. All web tests pass (259), and the e2e tests launcher, settings, and wm pass.
