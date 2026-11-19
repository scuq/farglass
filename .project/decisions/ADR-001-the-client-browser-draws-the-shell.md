---
id: ADR-001
title: The client browser draws the shell
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
rcd sends the pixels of a GNOME session. It depends on Mutter internals, software encode adds 21 ms to 101 ms for each frame, and text is not sharp.

## Decision
The shell (panel, window manager, launcher, terminal, settings) is a web app. The client browser draws it. The host runs no GNOME and no KDE. The client browser runs on a remote computer. A local kiosk is not in scope.

## Consequences
The shell has no video delay. The project must write its own window manager and apps. Linux GUI apps need a different path (ADR-007).
