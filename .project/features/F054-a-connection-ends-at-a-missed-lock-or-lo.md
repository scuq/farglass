---
id: F054
title: A connection ends at a missed lock or logout and at the cookie expiry
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
No connection keeps its channels after a lock, a logout, or the end of its cookie.

## Requirements
- On a lagged presence event, the connection checks the lock and the logout state of its user again
- A connection from a cookie ends at the expiry of the cookie
- Review findings 6 and 7 in docs/threats.md

## Acceptance
