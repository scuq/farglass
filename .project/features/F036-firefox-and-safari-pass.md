---
id: F036
title: Firefox and Safari pass
status: done
phase: P06
tags: []
decisions: [ADR-006]
files: [spikes/e2e/browsers.mjs, web/src/apps/settings/pages/Appearance.tsx, web/src/apps/settings/pages/Keyboard.tsx, README.md]
---

## Goal
The shell works in Firefox and Safari with documented limits.

## Requirements
- Fallbacks for Keyboard Lock and Window Management
- End-to-end tests on Firefox and WebKit

## Acceptance
browsers.mjs passes in Playwright Firefox and WebKit: login, echo, reload with cookie, terminal, window drag, dark theme, gnome-calculator with keys, animation as H.264 video (decoded in both), logout, no page error or CSP violation. Fallbacks: no WebTransport uses the WebSocket; no VideoDecoder or unsupported H.264 gives tiles; Keyboard Lock and Window Management are not used, and the shell shortcuts avoid browser-reserved keys. Fixed: two Settings windows shared radio groups. Limits in README > Browsers.
