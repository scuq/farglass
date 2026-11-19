---
id: F003
title: farglassd serves the shell through TLS
status: done
phase: P01
tags: []
decisions: [ADR-003, ADR-012, ADR-014, ADR-016]
files: [crates/farglassd/, packaging/farglassd.conf]
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
- With no certificate in the configuration, farglassd makes a self-signed ECDSA certificate that is valid for 30 days, and makes a new one when the old one expires in less than one day.
- cert and key in the configuration file, --tls-cert and --tls-key, or FARGLASSD_TLS_CERT and FARGLASSD_TLS_KEY load PEM files. farglassd loads changed files again each hour and after SIGHUP, and keeps the old certificate after an error.
- One TLS port serves HTTP/2 and HTTP/1.1 through ALPN. curl and the tests confirm the two versions.
- Shell files have cache headers: immutable for /assets/, no-cache for other files, no-store for errors. Responses use Brotli or gzip.
- /ws accepts WebSocket through HTTP/1.1 and through HTTP/2 extended CONNECT. It sends the hello message, refuses each channel, and closes the connection after a protocol violation.
- packaging/farglassd.conf has all options. A test parses it.
- 27 tests of farglassd pass.
