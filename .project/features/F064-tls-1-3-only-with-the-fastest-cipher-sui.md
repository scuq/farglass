---
id: F064
title: TLS 1.3 only with the fastest cipher suites
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglassd/src/tls.rs, crates/farglassd/tests/server.rs, README.md, docs/threats.md]
---

## Goal
farglassd speaks only TLS 1.3, with the cipher suites and key exchanges that are fastest on usual hardware. Speed comes before a larger key size.

## Requirements
- TLS 1.3 only on TCP and on QUIC
- Cipher suites in this order: TLS13_AES_128_GCM_SHA256, TLS13_CHACHA20_POLY1305_SHA256 (for clients with no AES instructions). No AES-256
- Key exchange: X25519, then secp256r1 for compatibility
- Tests: a TLS 1.2 client is refused; the server picks AES-128-GCM when the client offers it
- README and docs/threats.md

## Acceptance
tls::fast_provider gives aws-lc-rs with TLS13_AES_128_GCM_SHA256 and TLS13_CHACHA20_POLY1305_SHA256 and the groups X25519 and secp256r1; TCP and QUIC use it with TLS 1.3 only. Test only_tls_1_3_with_aes_128_gcm_first_and_x25519: TLS 1.3, AES-128-GCM, and X25519 are chosen; a TLS 1.2 client and a client with only AES-256 are refused. e2e login and browsers (Firefox, WebKit) pass.
