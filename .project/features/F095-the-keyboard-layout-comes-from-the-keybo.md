---
id: F095
title: The keyboard layout comes from the keyboard in Chromium, US otherwise
status: done
phase:
tags: [shell, surface]
decisions: []
files: [web/src/keys/layout.ts, web/src/keys/layout.test.ts, web/src/keys/scheme.ts, web/src/apps/settings/pages/Keyboard.tsx, web/src/wm/Desktop.tsx, CHANGELOG.md]
---

## Goal
The default layout of the Linux apps is the real layout of the keyboard, not a guess from the language.

## Requirements
- In a browser with navigator.keyboard.getLayoutMap (Chromium), the shell finds the layout from the characters of the keys, and asks again when the window gets the focus
- In a different browser, or when the call fails or no layout matches, the layout is US until the user selects one
- No layout comes from the browser language; F094 suggested one from navigator.languages

## Acceptance
layout.test.ts finds us, gb, de, ch, ch(fr), fr, be, se, es, ru, and Dvorak from layout maps; gives US with no Keyboard API also for de-AT, and when the call fails; keeps a selected layout. The type check and the 281 web tests pass. Not yet confirmed in Chromium.
