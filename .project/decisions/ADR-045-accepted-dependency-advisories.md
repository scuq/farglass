---
id: ADR-045
title: Accepted dependency advisories
status: accepted
date: 2026-11-21
tags: [security]
superseded_by:
files: []
---

## Context
cargo audit (2026-11-21) found RUSTSEC-2023-0071 in rsa 0.10.0-rc.19, through webauthn-rs 0.6.2 and crypto-glue 0.2.2. crypto-glue needs the rsa feature sha2, which rsa 0.10.0 removed: no update is possible until webauthn-rs moves. The other warnings: cgmath 0.18 (through smithay 0.7) is unmaintained and has an unsound swap_columns, and chrono is unmaintained but is not in the build for Linux. npm audit finds nothing.

## Decision
RUSTSEC-2023-0071 stays ignored in .cargo/audit.toml: the Marvin attack needs RSA decryption with a private key, and farglassd only verifies WebAuthn signatures with public keys. farglass calls no swap_columns of cgmath. Each new advisory fails cargo audit. When webauthn-rs releases a version with rsa 0.10, update and remove the ignore.

## Consequences
cargo audit and npm audit run before each release and pass. A new use of RSA decryption, or of a cgmath matrix, needs a new review of this decision.
