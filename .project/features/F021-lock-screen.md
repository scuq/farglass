---
id: F021
title: Lock screen
status: planned
phase: P04
tags: []
decisions: [ADR-004]
files: [crates/farglassd/, web/src/]
---

## Goal
A locked session opens only after a PAM login.

## Requirements
- farglassd blocks all channels while the session is locked
- Lock from the session menu and after an idle time
- Follow the lock and unlock signals of logind

## Acceptance
