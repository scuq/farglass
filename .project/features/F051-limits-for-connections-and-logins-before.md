---
id: F051
title: Limits for connections and logins before authentication
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglass-login/src/audit.rs, crates/farglass-login/src/limiter.rs, crates/farglass-login/src/server.rs, crates/farglass-login/src/main.rs, crates/farglass-login/src/client.rs, crates/farglass-proto/src/login.rs, crates/farglassd/src/device.rs, crates/farglassd/src/connection.rs, crates/farglassd/src/agents.rs, crates/farglassd/src/http.rs, crates/farglassd/src/ws.rs, crates/farglassd/src/quic.rs, crates/farglassd/src/lock.rs, packaging/systemd/farglassd.service, README.md, docs/threats.md]
---

## Goal
An attacker with no password cannot stop the logins of other users or guess one password from many addresses.

## Requirements
- farglassd closes a connection with no hello in 10 s, and a connection with no user after a login idle time
- A cap on connections and login channels for each address, with IPv6 addresses grouped by /64
- farglass-login checks the block of the user when PAM knows the username, before the password, and refuses a correct password while the user is blocked
- An abandoned attempt counts toward the block of the address
- One deadline of 60 s for the whole login, from the start to the session, in place of 60 s for each prompt
- A browser gets a long-lived device cookie after a successful login. Some login places, for example 8 of 32, are kept for browsers with a valid device cookie
- A cap on the pending agent connections for each uid; LimitNOFILE raised in the farglassd unit
- Each event of these limits goes to the journal at level warn, with the address, the username if known, and the reason: a hello timeout, an idle timeout, an address cap, a full login pool, a login deadline, a blocked address, a blocked user, an abandoned attempt
- The log lines have a fixed form that fail2ban can match, for example: login refused: reason=address-cap addr=203.0.113.7 user=-
- README tells the administrator how to see these events with journalctl and gives a fail2ban filter
- Review findings 2, 3, and 10 in docs/threats.md

## Acceptance
farglass-login groups IPv6 by /64, refuses a blocked user before the password and after a correct password, counts abandoned and deadline attempts, caps 4 logins per address, ends each login after 60 s, and keeps 8 of 32 places for browsers with a device cookie. farglassd closes a connection with no hello in 10 s and with no user after 10 minutes, caps 16 connections with no user per address and 4 agent links per uid, and sets the HMAC device cookie at POST /session. A full limiter keeps active blocks (ADR-042). Each event goes to farglass::security in the ADR-043 form; README gives journalctl and fail2ban. Tests: farglass-login login.rs (5 new), farglassd login.rs (3 new), agents.rs (1 new), audit, limiter, device unit tests; e2e login and passkey pass.
