---
id: F019
title: polkit authentication agent
status: planned
phase: P04
tags: []
decisions: [ADR-009]
files: [crates/farglass-session/src/polkit.rs, web/src/]
---

## Goal
A polkit prompt shows in the shell.

## Requirements
- Register as the authentication agent of the session
- Relay the PAM conversation of polkit to a dialog box in the shell
- Cancel when the client is not connected

## Acceptance
