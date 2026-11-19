---
id: ADR-003
title: Three host programs with privilege separation
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
In rcd the network daemon runs as root. A web server has a larger attack surface than the rcd protocol.

## Decision
farglassd listens on the network and runs as an unprivileged user. farglass-login runs as root and does only the PAM login and the session start. farglass-session, the agent, runs as the user. The agent connects to /run/farglass/agent.sock, and farglassd compares the peer uid from SO_PEERCRED with the user who logged in.

## Consequences
A defect in the web server does not give root. There are two links to keep stable: farglassd to farglass-login, and farglassd to the agent.
