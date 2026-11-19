---
id: F003
title: farglassd serves the shell through TLS
status: planned
phase: P01
tags: []
decisions: [ADR-003]
files: [crates/farglassd/]
---

## Goal
The client browser loads the shell from farglassd.

## Requirements
- A self-signed certificate on first start, or a configured certificate
- HTTP/1.1 and HTTP/2 on TCP through ALPN
- Static shell files with cache headers
- A WebSocket endpoint for the channel protocol, over HTTP/2 (RFC 8441) and HTTP/1.1
- Configuration file packaging/farglassd.conf

## Acceptance
