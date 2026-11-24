---
id: F104
title: Settings: filter, cards, and narrow layout
status: done
phase: P03
tags: [settings, shell]
decisions: []
files: [web/src/apps/settings/, web/src/theme/lcars.css, spikes/e2e/settings.mjs, README.md, CHANGELOG.md]
---

## Goal
The settings app has a filter that finds a setting on each page, shows its settings in cards, and fits a narrow window.

## Requirements
- A list of cards with a title and keywords for each page, with no DOM code, and the match rule of the launcher
- A filter above the list of pages: with text, the window shows the matching cards of all pages, or No settings match
- Escape clears the filter, a page from the list clears it, and Ctrl+F in the window goes to it
- The forms of a card share three columns, thus each text input has the same width
- The window is a size container: below 40rem the pages are pills at the top, below 30rem a label is above its input
- A table scrolls in its own box, and the mark This session does not break
- Each id, name, data attribute, and button text that a test uses stays

## Acceptance
The Settings window has a filter above the list of pages. With text, it shows the matching cards of all pages below the title of each page, or No settings match. Escape clears it, a page from the list clears it and opens the page, and Ctrl+F goes to it. Each page shows its settings in cards from one list of cards with keywords. The forms of a card share three columns: the inputs for the hostname and the display name have the same width, also their buttons. The window is a size container: below 40rem the filter and the pages are at the top as pills, below 30rem a label is above its input, and a table scrolls in its own box. Tests: sections.test.ts checks the list and the match rule, settings.test.tsx checks the filter, Escape, and the list of pages. settings.mjs passed in Chromium on a real host: the filter, the equal widths, the layout of a narrow window, and no horizontal scroll on four pages. Not tested: the line break of the mark This session in a browser, because a dev_user session has no mark; Ctrl+F in a browser.
