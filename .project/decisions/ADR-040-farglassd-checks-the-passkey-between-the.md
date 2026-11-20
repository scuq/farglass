---
id: ADR-040
title: farglassd checks the passkey between the PAM login and the session start
status: accepted
date: 2026-11-20
tags: [security, login]
superseded_by:
files: []
---

## Context
PAM runs in farglass-login as root. A WebAuthn check needs the origin of the page and a store, and the session must not start before the second factor passes. farglassd is unprivileged, sees the checked Origin header, and has a state directory.

## Decision
After PAM accepts the password, farglass-login sends a verified event and waits for a second-factor answer before the local-session step and the session start. farglassd answers it: with no passkey it accepts at once; with a passkey it asks the shell on the login channel and checks the passkey with webauthn-rs, or a recovery code. A refused answer counts as a failed login for the rate limiter. farglassd keeps the passkeys in STATE_DIR/passkeys/UID.json with mode 0600, and only SHA-256 hashes of the recovery codes. A passkey belongs to the host name of the page; an IP address cannot have one. The shell manages the passkeys on a passkeys channel that farglassd serves, not the agent.

## Consequences
A stolen password alone does not open a session of a user with a passkey. farglassd depends on webauthn-rs and OpenSSL. A user who opens farglass with a different host name needs a recovery code. An administrator removes the file of a user to remove the second factor. A session cookie can add or remove passkeys with no new check.
