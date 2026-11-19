---
id: F007
title: Login screen in the shell
status: done
phase: P02
tags: []
decisions: [ADR-004, ADR-018, ADR-019]
files: [web/src/, crates/farglassd/src/login.rs, crates/farglassd/src/connection.rs, crates/farglass-proto/src/login.rs, spikes/e2e/, docs/protocol.md]
---

## Goal
The shell shows each PAM prompt and sends the answer.

## Requirements
- Prompts with and without echo, information and error messages
- A new prompt for a one-time code after the password
- A clear message when the rate limiter blocks the login
- The login runs on a channel of kind login. After a success, the connection has the user, and its channels go to the agent of the user

## Acceptance
- The login screen shows each PAM prompt in its own field: a text field when PAM shows the answer, a password field when not. Information messages and error messages show above the field.
- A one-time code after the password gets a new field with autocomplete one-time-code. A web test confirms the sequence of a password and a code.
- A block shows 'Too many failed logins. Try again in N s.' and disables the new attempt until the time ends.
- The login runs on a channel of kind login. After a success, the connection has the user, and its channels go to the agent of that uid. 7 Rust tests with a real farglass-login confirm it.
- spikes/e2e/login.mjs logs in through Chromium, first with a wrong password and then with the correct one, and then uses the echo channel.
- The confirm step for a local graphical session moved to F008, which detects the local session.
