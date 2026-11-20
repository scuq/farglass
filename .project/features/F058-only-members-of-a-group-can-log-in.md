---
id: F058
title: Only members of a group can log in
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglass-login/src/server.rs, crates/farglass-login/src/main.rs, crates/farglass-login/src/client.rs, crates/farglass-proto/src/login.rs, crates/farglassd/src/connection.rs, crates/farglassd/src/server.rs, packaging/install-local.sh, spikes/e2e/lib.mjs, README.md, docs/threats.md]
---

## Goal
Only local users in one group can log in to farglass. A user outside the group never gets a session, and the web shows no difference to a wrong password. Only the journal tells the administrator the cause.

## Requirements
- farglass-login has the option --allowed-group NAME, default farglass-users. The group farglass of the sockets stays separate
- farglass-login checks the membership with getgrouplist, primary and supplementary groups, after PAM. PAM runs for each user, so the time of the answer does not show the membership
- A user outside the group gets the same failure text, the same delay, and the same block count as a wrong password. The login never reaches the passkey step, so the passkey step does not show the membership
- If the group does not exist, farglass-login refuses each login and logs an error at start
- The reconnect with a session cookie checks the membership again: a user removed from the group loses access at the next connection
- The journal gets a warn line in the form of F051: login refused: reason=not-in-group addr=203.0.113.7 user=bob
- install-local.sh creates the group and adds the session user. README tells the administrator how to add a user: usermod -aG farglass-users USER
- Tests: a member logs in; a user outside the group with the correct password gets the text of a wrong password and the log line; a missing group refuses each login
- docs/threats.md: the group in the login controls

## Acceptance
farglass-login --allowed-group (default farglass-users) checks the primary and supplementary groups with getgrouplist after PAM and before the passkey step. A user outside the group gets the text, delay, and block count of a wrong password; the journal gets reason=not-in-group. A missing group or a read error refuses each login (ADR-042) and logs an error at start. farglassd asks farglass-login (LoginRequest::Member) at each connection with a session cookie and gives the login screen and removes the sessions of a user outside the group; no answer refuses. install-local.sh --user adds users and warns if the group is empty. Tests: 5 farglass-login tests, 1 farglassd test with a fault case; e2e login passes.
