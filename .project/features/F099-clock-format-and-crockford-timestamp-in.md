---
id: F099
title: Clock format and crockford timestamp in the panel
status: done
phase: P04
tags: [shell, settings]
decisions: []
files: [web/src/clock.ts, web/src/clock.test.ts, web/src/panel/Clock.tsx, web/src/panel/Panel.tsx, web/src/panel/panel.css, web/src/theme/lcars.css, web/src/apps/settings/pages/DateTime.tsx, web/src/apps/settings/Settings.tsx, web/src/apps/settings/pages.ts, CHANGELOG.md, README.md]
---

## Goal
The user selects a 24-hour or a 12-hour clock, and can show the time also as a crockford timestamp.

## Requirements
- Settings > Date and time > Clock in the panel: the hour format (the language of the browser, 24 hours, or 12 hours), and a check box for the crockford timestamp
- The crockford timestamp follows https://github.com/scuq/crockford-timestamp: five Crockford Base32 characters in UTC, with +N after 2051, and no code out of 2020 to 3043
- The tooltip of the code gives the UTC start of its 2-minute interval

## Acceptance
Settings > Date and time > Clock in the panel selects the format of the browser language, 24 hours, or 12 hours, and turns on the crockford timestamp next to the time. The code follows the specification of scuq/crockford-timestamp; clock.test.ts covers each example of the specification, the range limits, the 2-minute start, the hour formats, and the parse. A Playwright run on a demo instance checked 12 hours, 24 hours, the code against an independent encode, and the settings after a reload.
