---
id: F102
title: System tray: watcher, icons, and activate
status: done
phase: P03
tags: [tray, panel]
decisions: [ADR-054]
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, crates/farglass-surface/src/cursor.rs, crates/farglass-session/src/tray.rs, crates/farglass-session/src/launch.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/lib.rs, crates/farglass-session/src/main.rs, crates/farglass-session/tests/tray.rs, web/src/panel/Tray.tsx, web/src/panel/tray.ts, web/src/panel/tray.test.ts, web/src/panel/Panel.tsx, web/src/panel/Panel.test.tsx, web/src/panel/panel.css, web/src/theme/lcars.css, web/src/wm/Desktop.tsx, README.md, CHANGELOG.md]
---

## Goal
A program that closes to the tray shows an icon in the panel, and a click on the icon brings its window back.

## Requirements
- The agent owns org.kde.StatusNotifierWatcher on the session bus and waits in the queue for the name
- An item registers by bus name or by object path, and goes away when its owner leaves the bus
- The panel shows one button for each item, with its icon, title, and status, left of the keyboard indicator
- Left click sends Activate, middle click sends SecondaryActivate, the wheel sends Scroll
- The icon comes from IconPixmap or from IconName with IconThemePath; the attention icon shows when the status is NeedsAttention
- Limits: 64 items, a pixmap side of 512, icon files of .svg and .png only
- No key in farglassd.conf

## Acceptance
The agent owns org.kde.StatusNotifierWatcher and waits in the queue for the name. An item registers by bus name or by object path and goes when its owner leaves the bus. The panel shows one button for each item, left of the keyboard indicator, with its icon, title, and status. A click sends Activate, the middle button SecondaryActivate, and the wheel Scroll. The icon comes from IconName with IconThemePath, from the hicolor theme and then the other themes, or from IconPixmap as a PNG file, and the attention icon shows with NeedsAttention. Limits: 64 items, a pixmap side of 512, .svg and .png files only. No key in farglassd.conf. Tests: tray.rs unit tests check the registration, the icons, the limits, and the store. tests/tray.rs registers fake items by name and by path on the session bus and checks a new icon, the three requests, and the removal. tray.test.ts and Panel.test.tsx check the buttons and the requests. tray.mjs checks the icon and a click in Chromium. A real libayatana indicator registered, gave its icon, and went away in a manual run. Not tested: Bitwarden and Seafile. That a click brings their window back is not proven on a host.
