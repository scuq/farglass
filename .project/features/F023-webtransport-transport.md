---
id: F023
title: WebTransport transport
status: planned
phase: P05
tags: []
decisions: [ADR-005]
files: [crates/farglassd/, web/src/proto/]
---

## Goal
Channels on independent streams, so a late frame does not block input.

## Requirements
- HTTP/3 endpoint in farglassd
- Alt-Svc header on TCP responses, so the browser finds the HTTP/3 endpoint
- Fall back to WebSocket when WebTransport is not available
- A certificate plan for self-signed hosts: short life and a pinned hash

## Acceptance
