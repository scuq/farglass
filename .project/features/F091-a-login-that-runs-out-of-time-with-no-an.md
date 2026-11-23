---
id: F091
title: A login that runs out of time with no answer does not count as a failure
status: done
phase:
tags: [login, security]
decisions: []
files: [crates/farglass-login/src/server.rs, crates/farglass-login/tests/login.rs, CHANGELOG.md]
---

## Goal

## Requirements

## Acceptance
A login that reaches the 60 s deadline with no answer does not count toward the block of the address or the user; one with an answer still counts. The login test covers both cases.
