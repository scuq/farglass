---
id: F072
title: LCARS tint for the shell icons
status: done
phase:
tags: [web, ui]
decisions: [ADR-049]
files: [web/index.html, web/src/theme/lcars.css, web/src/launcher/hostApps.ts, web/src/launcher/hostApps.test.ts, web/src/panel/Panel.tsx, web/src/panel/Launcher.tsx, web/src/wm/WindowFrame.tsx, web/src/wm/Overview.tsx, web/src/wm/LayoutPicker.tsx]
---

## Goal
The line icons of the shell match the LCARS theme.

## Requirements
- In the LCARS theme, the shell icons have the accent color, and are dark on a bar in the accent color
- Icons of host apps and the farglass icon keep their colors
- Light and dark themes do not change

## Acceptance
isShellIcon marks the icons of the shell apps and the calculator, not the farglass icon or a host icon (hostApps.test.ts). An SVG filter in index.html paints the marked icons in --focus, or --on-focus on an accent bar. A Chromium screenshot in dev_user mode shows amber icons in the launcher and panel, and dark icons on the amber selection and the pressed task button. All web tests pass.
