---
id: F034
title: Passkeys as a second factor
status: done
phase: P06
tags: []
decisions: [ADR-040]
files: [crates/farglassd/src/passkeys.rs, crates/farglassd/src/login.rs, crates/farglassd/src/connection.rs, crates/farglass-login/src/server.rs, crates/farglass-proto/src/login.rs, crates/farglass-proto/src/message.rs, web/src/passkey.ts, web/src/LoginScreen.tsx, web/src/apps/settings/pages/Passkeys.tsx, spikes/e2e/passkey.mjs]
---

## Goal
A user can add a passkey and must use it after the PAM login.

## Requirements
- WebAuthn registration and login
- Storage for each user on the host
- A recovery path for a lost passkey

## Acceptance
PAM login then a passkey step before the session starts (farglass-login waits for second-factor; farglassd checks with webauthn-rs). Settings > Passkeys adds and removes passkeys and makes 8 one-time recovery codes. Passkeys in STATE_DIR/passkeys/UID.json mode 0600, code hashes only; admin removes the file to reset. passkey.mjs passes with a virtual authenticator: add, passkey login, recovery code once, used code fails; login, settings, echo, wm e2e and all unit tests pass.
