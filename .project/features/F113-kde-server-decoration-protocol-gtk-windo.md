---
id: F113
title: KDE server decoration protocol: GTK windows with no header bar get the farglass frame only
status: done
phase:
tags: [surfaces, wayland]
decisions: []
files: [crates/farglass-surface/src/state.rs, crates/farglass-surface/tests/client.rs, README.md, CHANGELOG.md]
---

## Goal
A GTK window with no header bar shows only the frame of the shell, not a second title bar.

## Requirements

## Acceptance
The compositor offers org_kde_kwin_server_decoration_manager with the default mode Server, next to xdg-decoration; the default handler accepts the mode that a window asks for. Tests: client.rs checks that both globals are offered. By hand with the serve example: a plain GTK 3 window asked for mode 2 (server), got it, and its buffer was 300x120 with the geometry 300x120, thus no title bar and no shadow of GTK. Not tested: GNOME Terminal with headerbar false, and a window with a header bar.
