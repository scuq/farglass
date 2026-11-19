---
id: F023
title: WebTransport transport
status: done
phase: P05
tags: []
decisions: [ADR-036, ADR-005, ADR-016]
files: [crates/farglassd/src/quic.rs, crates/farglassd/src/http.rs, crates/farglassd/src/server.rs, crates/farglassd/src/config.rs, web/src/proto/webtransport.ts, web/src/proto/connection.ts, web/src/App.tsx, spikes/e2e/transport.mjs]
---

## Goal
Channels on independent streams, so a late frame does not block input.

## Requirements
- HTTP/3 endpoint in farglassd
- Alt-Svc header on TCP responses, so the browser finds the HTTP/3 endpoint
- Fall back to WebSocket when WebTransport is not available
- A certificate plan for self-signed hosts: short life and a pinned hash

## Acceptance
farglassd serves HTTP/3 and WebTransport on the UDP port of its TCP port (h3, h3-quinn, h3-webtransport). Each channel has its own bidirectional stream with the same frames. POST /transport gives a single-use 30 s ticket, the port, and the SHA-256 hash of a separate 10-day ECDSA certificate for self-signed hosts (renewed 2 days before its end). Alt-Svc goes out with an administrator certificate only. The shell tries WebTransport with a 3 s limit and falls back to the WebSocket (no WebTransport in the browser, http3 = false, a different page port, or a failure). Tested by Rust tests (pinned certificate renewal, single-use tickets, an HTTP/3 client for GET / and POST /transport with the hash of the presented certificate, Alt-Svc with and without an administrator certificate), vitest tests of the stream mapping, and spikes/e2e/transport.mjs: echo 1.0 ms on WebTransport, a terminal on it, and an echo during terminal output of p50 3.4 ms on WebTransport against 33 ms on the WebSocket.
