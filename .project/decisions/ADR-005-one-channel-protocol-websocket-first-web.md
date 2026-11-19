---
id: ADR-005
title: One channel protocol, WebSocket first, WebTransport for surfaces
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
The shell needs terminals, D-Bus, files, and later video data on one connection. WebSocket works in all browsers. A late video frame on WebSocket blocks input.

## Decision
One connection carries numbered channels: pty, dbus, fs, launch, notify, surface. A frame is a binary header (channel, type, length) and a payload. Control messages are JSON. Terminal data and surface data are raw bytes. farglass-proto defines the types and generates the TypeScript types. The first transport is WebSocket. The surface milestone adds WebTransport.

## Consequences
The terminal and the settings need no HTTP/3. The protocol must not depend on one ordered stream.
