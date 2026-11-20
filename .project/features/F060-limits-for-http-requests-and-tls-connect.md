---
id: F060
title: Limits for HTTP requests and TLS connections for each address
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglassd/src/connection.rs, crates/farglassd/src/server.rs, crates/farglassd/src/quic.rs, crates/farglassd/src/http.rs, crates/farglassd/src/cookie.rs, crates/farglassd/tests/server.rs, README.md, docs/threats.md]
---

## Goal
An attacker with no password cannot use up the file descriptors or the memory of farglassd with TCP connections, TLS handshakes, or ticket requests.

## Requirements
- A cap on TCP connections at the same time for each address, IPv6 grouped by /64, before the TLS handshake
- A rate limit on POST /session and POST /transport for each address, and a cap on the tickets in memory
- Each refusal goes to the security log (ADR-043)
- Review finding 14 in docs/threats.md

## Acceptance
farglassd caps TCP and QUIC connections at 64 per address group before the TLS handshake (the place goes with the connection), rate-limits POST /session and POST /transport with a bucket of 30 that fills at 1 per second for each address group (429 and a security log line; a full table of 10 000 groups refuses a new one), and keeps at most 10 000 tickets in each ticket table, losing the oldest. Tests: rate limit unit tests; server.rs one_address_gets_a_limited_number_of_tcp_connections and the_ticket_endpoints_have_a_rate_limit. e2e term passes; e2e login passed 3 of 4 runs (one getAttribute timeout, cause not found).
