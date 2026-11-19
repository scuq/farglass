---
id: ADR-019
title: The login runs on a channel of the browser connection
status: accepted
date: 2026-11-19
tags: [login, protocol]
superseded_by:
files: [crates/farglassd/src/login.rs, crates/farglassd/src/connection.rs, web/src/LoginScreen.tsx]
---

## Context
The shell needs a login before it can reach the agent (ADR-004). The shell already has one connection with channels (ADR-005). F009 adds a session cookie, and F010 adds the connection again after a break.

## Decision
- The login is a channel of kind login on the browser connection. farglassd serves it and relays each LoginEvent of farglass-login as a control message. The shell answers with LoginAnswer.
- farglassd starts each attempt with no username, thus the PAM modules ask for it. The address of the browser goes to farglass-login for the rate limiter.
- After a success, the connection has the user. New channels go to the agent of that uid. Before, farglassd refuses each other channel with access-denied.
- The hello message of farglassd has the user when the connection already has one, for example with dev_user.
- One login channel is open at a time. A connection with a user refuses a new login.
- The shell shows only what PAM asks. It sets autocomplete to username, current-password, or one-time-code from the prompt.

## Consequences
Each new browser connection must log in again until F009 adds the session cookie. Then farglassd sets the user of a connection from the cookie and sends it in the hello message, and the shell skips the login screen. The confirm step for a local graphical session (ADR-015) is a new LoginEvent in F008.
