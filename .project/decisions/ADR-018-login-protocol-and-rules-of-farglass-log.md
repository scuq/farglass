---
id: ADR-018
title: Login protocol and rules of farglass-login
status: accepted
date: 2026-11-19
tags: [login, security]
superseded_by:
files: [crates/farglass-login/, crates/farglass-proto/src/login.rs, packaging/pam.d/farglass, packaging/systemd/]
---

## Context
farglass-login runs as root (ADR-003) and must give a small attack surface. farglassd relays the PAM conversation (ADR-004). A compromised farglassd must not be able to try passwords without limit.

## Decision
- One connection to /run/farglass/login.sock is one login attempt. The messages are LoginRequest (start, answer) and LoginEvent (prompt, info, error, success, failure, blocked), as JSON in control frames on channel 0.
- farglass-login keeps the rate limiter, not farglassd: for each address and each user, 5 free failures, then a block of 30 seconds that doubles up to 15 minutes, and at most 10000 counters.
- After a failure, farglass-login waits 2 seconds before the answer. The message does not tell if the user exists. Only an account failure after a correct password gives its cause.
- farglass-login refuses root by default. --allow-root permits it.
- At most 32 attempts run at the same time. Each one has a PAM thread, and each prompt has a limit of 60 seconds.
- systemd starts farglass-login through a socket unit with mode 0660 and the group farglass. The service has no network (PrivateNetwork) and a strict sandbox.
- Answers are in Zeroizing memory and are cleared after use.
- All unsafe code of farglass-login is in the modules pam and activation.

## Consequences
A PAM module that needs the network needs a drop-in file that removes PrivateNetwork. The limiter state is lost when farglass-login restarts. farglassd trusts the uid in a success message, because only root can send it. The package must create the group farglass (F033). F007 relays the events to the shell, and F008 adds the session start to the same connection.
