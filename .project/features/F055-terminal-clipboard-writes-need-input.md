---
id: F055
title: Terminal clipboard writes need input
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
Output of a program, for example of a remote host over ssh, cannot write the clipboard of the client computer without the user.

## Requirements
- An OSC 52 write goes to the clipboard only within a short time after a key or a click in that terminal, or after a confirm
- Review finding 8 in docs/threats.md

## Acceptance
