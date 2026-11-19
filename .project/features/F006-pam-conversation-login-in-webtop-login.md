---
id: F006
title: PAM conversation login in farglass-login
status: planned
phase: P02
tags: []
decisions: [ADR-003, ADR-004]
files: [crates/farglass-login/, packaging/pam.d/farglass]
---

## Goal
farglass-login does a PAM login and relays each PAM prompt.

## Requirements
- Start from rcd/crates/rcdd/src/pam.rs and auth.rs
- Rate limiter for each address and each user
- Socket-activated, runs as root, has no network access
- A --pam-dir option for tests with no root

## Acceptance
