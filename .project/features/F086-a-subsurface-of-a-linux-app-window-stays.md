---
id: F086
title: A subsurface of a Linux app window stays in the frame
status: done
phase:
tags: [surface, wm]
decisions: []
files: [web/src/apps/linux/Linux.tsx, README.md, CHANGELOG.md]
---

## Goal
No part of a Linux app window covers the border of its frame. Menus can still reach past the frame.

## Requirements
- A subsurface of the window, or of such a subsurface, draws in the clipped area of the window (.linux-main)
- An xdg popup and each surface under it draw outside that area, as before

## Acceptance
A screenshot of the user showed the pixels of the tab strip and the toolbar of Brave over the right border of the focused frame while YouTube played in the smart mode. In the shell only popups and subsurfaces draw outside the clipped area; with no menu open, a Chromium subsurface covered the border. A probe in the redirect test (40 pointer moves on and off the video) found no change of class or style on the window or its parents and an amber border on all four sides, thus the fault is not the focus. After the change, the e2e test popups passes (suggestions and extension panel show, the menu reaches below the window, the submenu past its right edge), and the e2e test redirect passes with each step of the smart mode. Not tested: Brave with YouTube on the host of the user.
