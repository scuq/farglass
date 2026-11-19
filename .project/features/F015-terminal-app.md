---
id: F015
title: Terminal app
status: planned
phase: P03
tags: []
decisions: [ADR-012]
files: [web/src/apps/terminal/]
---

## Goal
A terminal window with a delay of less than 5 ms on loopback from key press to echo.

## Requirements
- xterm.js with the WebGL renderer behind a thin interface
- Copy and paste with OSC 52 and the Async Clipboard API
- Tabs or more than one window
- A speed test with the Performance API

## Acceptance
