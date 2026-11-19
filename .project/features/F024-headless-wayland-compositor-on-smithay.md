---
id: F024
title: Headless Wayland compositor on Smithay
status: planned
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-surface/]
---

## Goal
A compositor in the agent that gives each top-level window as one surface channel.

## Requirements
- xdg-shell, shm buffers, popups
- No desktop drawing. The shell is the only window manager
- Size, focus, and close come from the shell
- Buffer and damage for each commit

## Acceptance
