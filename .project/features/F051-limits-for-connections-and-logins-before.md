---
id: F051
title: Limits for connections and logins before authentication
status: planned
phase: P06
tags: [security]
decisions: []
files: []
---

## Goal
An attacker with no password cannot stop the logins of other users or guess one password from many addresses.

## Requirements
- farglassd closes a connection with no hello in 10 s, and a connection with no user after a login idle time
- A cap on connections and login channels for each address, with IPv6 addresses grouped by /64
- farglass-login checks the block of the user when PAM knows the username, before the password, and refuses a correct password while the user is blocked
- An abandoned attempt counts toward the block of the address
- A cap on the pending agent connections for each uid; LimitNOFILE raised in the farglassd unit
- Review findings 2, 3, and 10 in docs/threats.md

## Acceptance
