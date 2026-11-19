---
id: F031
title: Linux apps in the launcher
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-session/src/launch.rs, web/src/launcher/]
---

## Goal
The launcher lists desktop entries and starts them on the compositor.

## Requirements
- Read the desktop entries and icons of the host
- Each app in its own systemd --user scope
- A surface window shows in the panel as other windows do

## Acceptance
