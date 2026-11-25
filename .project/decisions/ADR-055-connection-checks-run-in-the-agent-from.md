---
id: ADR-055
title: Connection checks run in the agent, from a user file and an administrator file
status: accepted
date: 2026-11-25
tags: [network, agent]
superseded_by:
files: []
---

## Context
The browser cannot send ICMP or open a TCP connection to a host and port, and the connection that matters is the one of the host. A user needs to see at a glance if the internet, DNS, and the internal services that they need are available or blocked.

## Decision
The agent runs the checks as the user on a netcheck channel. The built-in checks are HTTPS to 1.1.1.1 and google.com, also through the proxy of https_proxy when it is set, and a DNS lookup of google.com with the system resolver. The user adds ICMP and TCP checks in $XDG_CONFIG_HOME/farglass/netchecks.toml; the administrator adds read-only checks in /etc/farglass/netchecks.toml. ICMP uses the unprivileged ping socket and falls back to the ping program. farglassd runs no checks.

## Consequences
A check shows the path of the host, not of the client. Each check has a timeout of at most 10 s, thus a full run ends in 10 s. A later check kind (HTTP, TLS) goes into the same file with a new kind value.
