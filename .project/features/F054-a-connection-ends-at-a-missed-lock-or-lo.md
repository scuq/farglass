---
id: F054
title: A connection ends at a missed lock or logout and at the cookie expiry
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglassd/src/connection.rs, crates/farglassd/src/cookie.rs, crates/farglassd/src/ws.rs, crates/farglassd/src/quic.rs, crates/farglassd/src/http.rs, README.md, docs/threats.md]
---

## Goal
No connection keeps its channels after a lock, a logout, or the end of its cookie.

## Requirements
- On a lagged presence event, the connection checks the lock and the logout state of its user again
- A connection from a cookie ends at the expiry of the cookie
- Review findings 6 and 7 in docs/threats.md

## Acceptance
A connection from a session cookie gets the end of the cookie (SessionStore::session, also through the WebTransport ticket as quic::Admission) and ends at that time; a login on the connection sets the end to now plus SESSION_LIFETIME. A lagged or closed presence subscription ends the connection; the shell connects again and gets the current state. Both log to farglass::security (cookie-expired, missed-events). Tests: farglassd login.rs a_connection_from_a_cookie_ends_at_the_end_of_the_cookie and a_connection_that_misses_user_events_ends.
