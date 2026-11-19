---
id: ADR-009
title: Administrator actions through polkit, no root bridge
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
Settings must change system services such as the network and the time. Cockpit starts a second, privileged bridge for this.

## Decision
farglass-session registers as the polkit authentication agent of the session and relays the prompt to the shell. The agent calls system services through D-Bus as the user.

## Consequences
No farglass program other than farglass-login runs as root. Settings can change only what a D-Bus service with a polkit policy offers.
