---
id: F044
title: Keybinding schemes with hints and an interactive tour
status: planned
phase: P03
tags: [shell]
decisions: [ADR-006]
files: [web/src/keys/, web/src/tour/]
---

## Goal
The shortcuts follow the conventions that the user knows, and the shell teaches them.

## Requirements
- Schemes for Windows and Mac conventions, selected from the operating system of the client by default
- Hints on the screen show the keys of the active scheme
- A binding never takes a key that input methods, screen readers, or apps use
- When the browser keeps a shortcut, the scheme has a second binding
- An interactive tour of the shortcuts

## Acceptance
