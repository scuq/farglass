---
id: F010
title: Connect again, logout, and idle timeout
status: planned
phase: P02
tags: []
decisions: [ADR-004]
files: [crates/farglassd/, crates/farglass-session/, web/src/]
---

## Goal
The session continues with no client and ends on logout or timeout.

## Requirements
- The agent keeps the list of open channels and windows
- The shell restores its state after it connects again
- Logout ends the session unit
- Idle logout after a configured time

## Acceptance
