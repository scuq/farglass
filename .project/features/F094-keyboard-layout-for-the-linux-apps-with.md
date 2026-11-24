---
id: F094
title: Keyboard layout for the Linux apps, with a default from the browser language
status: done
phase:
tags: [shell, surface]
decisions: []
files: [web/src/keys/layout.ts, web/src/keys/layout.test.ts, web/src/keys/scheme.ts, web/src/apps/settings/pages/Keyboard.tsx, web/src/wm/Desktop.tsx, crates/farglass-proto/src/message.rs, crates/farglass-session/src/surfaces.rs, crates/farglass-surface/src/state.rs, crates/farglass-surface/tests/client.rs, web/src/proto/generated/messages.ts, CHANGELOG.md]
---

## Goal
The Linux apps type the characters that are on the keys of the user, also with a keyboard that is not US.

## Requirements
- Settings > Keyboard has a layout list for the Linux apps; the default is the layout of the browser languages (navigator.languages), with the region first (de-CH gives ch, fr-CH gives ch(fr)), and US as the fallback
- The shell sends a keymap request on the surfaces channel at start and after each change; the agent accepts only XKB names
- The compositor changes the XKB keymap of the seat at once; an unknown layout keeps the keymap

## Acceptance
Settings > Keyboard lists the layouts and preselects the layout of the browser languages; layout.test.ts covers the language mapping and the id checks. The agent test passes a valid keymap request to the compositor and drops names that are not XKB names. The compositor test client gets a new keymap with French (Switzerland) after a ch(fr) request, and no keymap after an unknown layout.
