---
id: ADR-036
title: WebTransport with one stream for each channel, a ticket, and a pinned certificate
status: accepted
date: 2026-11-19
tags: [protocol, transport, security]
superseded_by:
files: []
---

## Context
ADR-005 adds WebTransport so that a late frame of one channel does not stop the others. A WebTransport request carries no cookie, a self-signed certificate of 30 days is too long for serverCertificateHashes (14 days at most), and HTTP/3 for the pages needs a trusted certificate.

## Decision
farglassd serves HTTP/3 with h3, h3-quinn, and h3-webtransport on the UDP port with the number of the TCP port, with the same router as TCP. Each channel uses its own bidirectional stream with the unchanged frames; the first frame of a stream selects its channel, and farglassd keeps the frames of channel 0 until that stream exists. POST /transport gives a single-use ticket of 30 s for the user of the cookie, the port, and the hashes. With a self-signed certificate, QUIC uses a separate in-memory ECDSA P-256 certificate of 10 days, renewed 2 days before its end. Alt-Svc goes out only with a certificate of the administrator. The shell tries WebTransport first, with a 3 s limit, and uses the WebSocket for 5 minutes after a failure, or when the page port is not the QUIC port.

## Consequences
The connection code of farglassd and the shell does not know the transport. h3-webtransport is an early crate and implements draft 02 of WebTransport over HTTP/3; a change of Chromium can need an update, and the fallback keeps the shell usable. Datagrams are on for the surfaces later. A proxy in front of farglassd gives only the WebSocket unless it passes UDP on the same port.
