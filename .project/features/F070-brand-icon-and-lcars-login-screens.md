---
id: F070
title: Brand icon and LCARS login screens
status: done
phase:
tags: [web, ui]
decisions: []
files: [web/index.html, web/src/brand/, web/src/LoginScreen.tsx, web/src/PinScreen.tsx, web/src/launcher/hostApps.ts, web/src/shell.css, web/src/apps/about/About.tsx]
---

## Goal
The farglass icon marks the shell, and the screens before the desktop have the LCARS look of the icon.

## Requirements
- The icon is the favicon
- The icon is the image of a window or app with no icon
- The login, lock, and PIN screens are in an LCARS frame in the colors of the icon
- Texts, ids, and roles of the login screens do not change; errors keep a text and the error color

## Acceptance
web/src/brand/farglass.svg is the favicon: dist/index.html links /assets/farglass-<hash>.svg. windowIcon gives the farglass icon to a window of an app with no icon (hostApps.test.ts). The login, lock, and PIN screens render in the LCARS frame with aria-hidden bars (login.test.tsx); a Chromium screenshot at 1000 px and 360 px width shows the frame with no horizontal scroll. All 32+ web test files pass, and the texts, ids, and roles of the login screens are unchanged.
