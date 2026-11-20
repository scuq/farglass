---
id: F059
title: A machine PIN before the web login
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglassd/src/pin.rs, crates/farglassd/src/connection.rs, crates/farglassd/src/main.rs, crates/farglassd/src/server.rs, crates/farglass-proto/src/message.rs, web/src/PinScreen.tsx, web/src/App.tsx, web/src/proto/connection.ts, spikes/e2e/pin.mjs, README.md, docs/threats.md]
---

## Goal
An optional PIN of 4 to 6 digits, one for the whole host, stops random input before the login. Until a connection gives the PIN, it cannot send a username, a password, a passkey, or a recovery code.

## Requirements
- Off by default. The administrator sets the PIN with a command, for example farglassd --set-pin, which writes a salted hash to a file of mode 0600. No PIN in farglassd.conf as plain text
- farglassd checks the PIN on the server: a connection opens no login channel and gets no prompt until the PIN is correct. The shell shows only the PIN field before that
- A reconnect with a valid session cookie does not ask for the PIN again
- The PIN check takes constant time and has a fixed delay of 1 s after a failure
- Five wrong PINs block the address, IPv6 grouped by /64, for 15 minutes, with the limits of F051
- Each success and each failure goes to the journal once, in the form of F051: pin accepted addr=203.0.113.7, pin refused: reason=wrong-pin addr=203.0.113.7, pin refused: reason=blocked addr=203.0.113.7
- The PIN is only a filter, not a factor: the password and the passkey stay necessary. README says this, and that 4 to 6 digits give no protection without the rate limit
- Tests: no login channel before the PIN; a correct PIN shows the login; a wrong PIN gives the log line and the delay; the block after five failures; no PIN asked when the feature is off
- docs/threats.md: the PIN in the login controls

## Acceptance
farglassd --set-pin writes STATE_DIR/pin (v1:salt:iterated SHA-256, mode 0600, owner of the state directory); --clear-pin removes it. With the file, the hello of a connection with no user has pin=true, and open_login is refused (AccessDenied) until ConnectionMessage::Pin is correct. A wrong PIN waits 1 s; five block the /64 or address for 15 min; a full table keeps blocks. A damaged or unreadable file requires the PIN and refuses each one (ADR-042). A cookie user gets no PIN. Each result goes to farglass::security once, with no PIN. The shell shows PinScreen before LoginScreen. Logs have no ANSI codes when stderr is not a terminal. Tests: pin.rs unit tests (8), farglassd login.rs (2), vitest (3), e2e pin passes.
