---
id: F062
title: The security log names only accounts of the host
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
A password typed in the username field does not go into the journal.

## Requirements
- A failed login logs the username only if the host has that account, else user=unknown
- The block of the user still counts the typed name
- Review finding 16 in docs/threats.md

## Acceptance
