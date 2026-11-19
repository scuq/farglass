---
id: ADR-013
title: One channel protocol, QUIC first with fallback to TCP
status: rejected
date: 2026-11-19
tags: [transport, quic, performance]
superseded_by:
files: [crates/farglassd/, crates/farglass-proto/, web/src/proto/]
---

## Context
The shell needs terminals, D-Bus, files, sound, and video data on one connection. ADR-005 used WebSocket first and added WebTransport only for surfaces. On one TCP stream, a late or lost packet blocks all channels, also input. QUIC streams are independent. Chromium, the primary target (ADR-006), supports HTTP/3 and WebTransport. Some networks block UDP, and some browsers do not support WebTransport.

## Decision
Use QUIC whenever possible, and fall back to TCP when QUIC is not available.
- One connection carries numbered channels: pty, dbus, fs, launch, notify, surface, audio. A frame is a binary header (channel, type, length) and a payload. Control messages are JSON. Terminal data and surface data are raw bytes. farglass-proto defines the types and generates the TypeScript types.
- The preferred transport is WebTransport over HTTP/3. Each channel has its own QUIC stream. Data that can be dropped, for example surface frames that a newer frame replaces, can use datagrams.
- The fallback transport is WebSocket over HTTP/2 (RFC 8441), then WebSocket over HTTP/1.1.
- farglassd listens on the same port on TCP and UDP. TCP serves HTTP/1.1 and HTTP/2 through ALPN. UDP serves HTTP/3. Each TCP response sends an Alt-Svc header for HTTP/3, so the browser also loads the static files through QUIC.
- The shell tries WebTransport first with a short timeout, then the fallback. On each connect again, it tries WebTransport again.
- The channel code does not know the transport. A transport interface on both sides hides it.

## Consequences
All channels get independent streams, not only surfaces. The transport interface must exist from the first channel (F002, F005), so WebTransport can come in with no change to the channels. The protocol must not depend on one ordered stream. Flow control must work on each stream and on the shared WebSocket. Tests must run each channel on both transports. The firewall must open UDP and TCP on the port. Self-signed hosts need a certificate plan for WebTransport: ECDSA, at most 14 days valid, a pinned hash. The HTTP/3 crates (quinn, h3, h3-webtransport, or wtransport) are younger than hyper.
