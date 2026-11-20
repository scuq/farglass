---
id: F061
title: The first passkey needs the password again
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglass-proto/src/login.rs, crates/farglass-proto/src/message.rs, crates/farglass-login/src/server.rs, crates/farglass-login/src/client.rs, crates/farglassd/src/passkeys.rs, crates/farglassd/src/connection.rs, web/src/apps/settings/pages/Passkeys.tsx, spikes/e2e/passkey.mjs, README.md, docs/threats.md]
---

## Goal
A person at an unlocked tab or with a stolen cookie cannot add a passkey to an account with no passkey and so lock out the user.

## Requirements
- The first registration asks for the password of the user through farglass-login, with the limits of F051
- Review finding 15 in docs/threats.md

## Acceptance
A user with no passkey gets PasskeyEvent::Password before a registration; the shell answers with ProofPassword, and farglassd checks it through farglass-login with LoginRequest::Start{check: true}: PAM with the limits, the block, and the group check of a login, no second factor, no session. No answer from farglass-login refuses. A password proof allows only the waiting change, not the 5-minute window of a passkey proof. Tests: farglass-login a_password_check_starts_no_session_and_counts_failures; farglassd the_first_passkey_needs_the_password_and_no_login_service_refuses_it; e2e passkey: a wrong password changes nothing, the right one adds the first passkey, and later changes still need the passkey.
