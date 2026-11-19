---
id: F041
title: Focus protection for notifications and new windows
status: planned
phase: P04
tags: [shell]
decisions: [ADR-001]
files: [web/src/wm/, web/src/notify/, crates/farglass-session/src/notify.rs]
---

## Goal
The desktop does not interrupt the user.

## Requirements
- Hold notifications while the user types or moves the pointer, and show them at a pause
- Urgent notifications show immediately
- A window that the user did not request opens with no focus and shows a mark in the panel
- A setting turns each rule on or off

## Acceptance
