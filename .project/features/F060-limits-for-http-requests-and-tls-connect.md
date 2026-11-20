---
id: F060
title: Limits for HTTP requests and TLS connections for each address
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
An attacker with no password cannot use up the file descriptors or the memory of farglassd with TCP connections, TLS handshakes, or ticket requests.

## Requirements
- A cap on TCP connections at the same time for each address, IPv6 grouped by /64, before the TLS handshake
- A rate limit on POST /session and POST /transport for each address, and a cap on the tickets in memory
- Each refusal goes to the security log (ADR-043)
- Review finding 14 in docs/threats.md

## Acceptance
