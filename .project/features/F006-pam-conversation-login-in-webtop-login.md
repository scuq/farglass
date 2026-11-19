---
id: F006
title: PAM conversation login in farglass-login
status: done
phase: P02
tags: []
decisions: [ADR-003, ADR-004, ADR-018]
files: [crates/farglass-login/, crates/farglass-proto/src/login.rs, packaging/pam.d/farglass, packaging/systemd/farglass-login.socket, packaging/systemd/farglass-login.service]
---

## Goal
farglass-login does a PAM login and relays each PAM prompt.

## Requirements
- Start from rcd/crates/rcdd/src/pam.rs and auth.rs
- Rate limiter for each address and each user
- Socket-activated, runs as root, has no network access
- A --pam-dir option for tests with no root

## Acceptance
- farglass-login does a PAM login and sends each prompt, information message, and error message as an event. The code comes from rcd pam.rs and auth.rs.
- The rate limiter blocks an address and a user after 5 failures, for 30 seconds that double up to 15 minutes. Tests confirm the block and the limit of 10000 counters.
- farglass-login takes its socket from systemd (tested with systemd-socket-activate) or creates it with --socket. The units packaging/systemd/farglass-login.socket and .service give root with no network and a strict sandbox. systemd-analyze verify accepts them.
- --pam-dir lets tests run with no root. 7 tests with a pam_exec stack confirm success, failure, a username prompt, the refusal of root, an unknown user, the block, and the stop of an attempt.
- packaging/pam.d/farglass has common-auth, an optional TOTP line, and the account checks.
