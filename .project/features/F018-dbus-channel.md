---
id: F018
title: dbus channel
status: planned
phase: P04
tags: []
decisions: [ADR-005, ADR-009]
files: [crates/farglass-session/src/dbus.rs, web/src/proto/]
---

## Goal
The shell calls methods, reads properties, and gets signals on the session bus and the system bus.

## Requirements
- An allowlist of bus names in the agent
- JSON form of D-Bus values
- Signal subscriptions that end with the channel

## Acceptance
