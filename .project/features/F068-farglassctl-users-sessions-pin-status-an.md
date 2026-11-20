---
id: F068
title: farglassctl users, sessions, PIN, status, and logs
status: planned
phase: P06
tags: [packaging]
decisions: [ADR-043, ADR-046]
files: []
---

## Goal
The administrator manages the users and the sessions of farglass with farglassctl.

## Requirements
- user list: members of farglass-users, with a web session or not, and passkeys yes or no
- user add USER, user remove USER
- user disable USER: removes the user from farglass-users and ends the web sessions; user enable USER adds the user back
- session list: the web sessions from logind; session logout USER: ends them
- pin set (reads the PIN with no echo), pin clear
- status: the units, the version, the listen address, the certificate, the PIN, the members
- logs [--security] [--follow]: journalctl of the units, or only farglass::security
- doctor: checks of the installation with a hint for each fault
- Tests: the parsers of loginctl, getent, and the log filter

## Acceptance
