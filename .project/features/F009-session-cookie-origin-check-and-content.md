---
id: F009
title: Session cookie, Origin check, and Content Security Policy
status: planned
phase: P02
tags: []
decisions: [ADR-003]
files: [crates/farglassd/]
---

## Goal
Only the browser that logged in can use the session.

## Requirements
- Cookie with HttpOnly, Secure, and SameSite=Strict
- Refuse a WebSocket connection with a different Origin
- Content Security Policy with no inline script, COOP and COEP headers

## Acceptance
