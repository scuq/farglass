---
id: F120
title: Host apps follow the dark or light theme of farglass
status: done
phase:
tags: [theme]
decisions: [ADR-059, ADR-015]
files: [crates/farglass-session/src/theme.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/main.rs, crates/farglass-session/src/lib.rs, crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, crates/farglass-proto/tests/vectors.json, web/src/hostTheme.ts, web/src/hostTheme.test.ts, web/src/wm/Desktop.tsx, web/src/proto/generated/messages.ts, spikes/e2e/theme.mjs, spikes/e2e/package.json, docs/protocol.md, README.md, CHANGELOG.md]
---

## Goal
The GTK and KDE apps of a web session use the dark or light theme of the shell, and the local desktop of the user keeps its own theme.

## Requirements
- ADR-059 gives the design.
- A channel kind theme: the shell sends {"type":"color-scheme","dark":true|false} at the start and after each change of the theme.
- The agent saves the values of the user before its first change, sets GSettings and kdeglobals, and restores them on SIGTERM and at a start with no web session.
- Unit tests with fake settings: the save happens once, a custom gtk-theme stays, the restore writes the values back and deletes the file, kdeglobals with no file before is deleted at the restore.
- Update docs/protocol.md, the README, and the changelog.

## Acceptance
The GTK and KDE apps of a web session follow the dark or light theme of the shell (ADR-059). The shell sends the resolved theme on a theme channel; the agent saves the values of the user once, sets GSettings color-scheme, gtk-theme (only for Adwaita), and the Breeze colors in kdeglobals with notifyChange, and restores them on SIGTERM; a crash leaves the saved file, and the original values stay. It does this only in a web session or with FARGLASS_FOLLOW_THEME=1. Tests: 6 theme unit tests with a fake backend, the hostTheme web test, the protocol vector, and npm --prefix spikes/e2e run theme with GSettings in a keyfile: LCARS gives prefer-dark and Adwaita-dark, Light gives prefer-light and Adwaita, and a SIGTERM brings back the values and deletes the file. The settings of the user stayed unchanged.
