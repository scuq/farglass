---
id: F097
title: Keyboard layout indicator in the panel
status: done
phase: P03
tags: [shell, keys]
decisions: []
files: [web/src/panel/KeyboardIndicator.tsx, web/src/panel/Panel.tsx, web/src/panel/panel.css, web/src/keys/layout.ts, web/src/keys/layout.test.ts, web/src/keys/scheme.ts, web/src/keys/scheme.test.tsx, web/src/wm/Desktop.tsx, web/src/apps/settings/pages/Keyboard.tsx, CHANGELOG.md, README.md]
---

## Goal
The panel shows the keyboard layout of the Linux apps, and a click switches it.

## Requirements
- A short code in the panel, for example US, DE, or AT, with the full name in the tooltip
- A menu: Automatic, the layouts used last, and More layouts that opens Settings > Keyboard
- With Automatic in Chromium, the shell asks the browser for the layout again each 3 s while the window has the focus, so a change in the operating system shows without a focus change

## Acceptance
The panel shows the layout code (US, AT, CH-FR) with the full name in the tooltip. The menu lists Automatic, the selected layout, and the recent layouts, four at most; a pick changes the setting, and Settings > Keyboard shows it. More layouts opens Settings > Keyboard. With Automatic, the shell asks the browser again each 3 s while the window has the focus. layout.test.ts and scheme.test.tsx cover the code and the recent layouts; a Playwright run on a demo instance checked the panel, the menu, and the settings.
