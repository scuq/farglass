---
id: ADR-044
title: Accepted risks after the agent link of the worker
status: accepted
date: 2026-11-20
tags: [security]
superseded_by:
files: []
---

## Context
ADR-041 accepted that a process of the user can replace the agent. F057 removes that risk: farglassd accepts an agent link only from the root session worker, and the agent is not dumpable. The other risks of ADR-041 stay.

## Decision
These risks stay: a process of the user can unlock the web lock screen through logind, as with GNOME; the D-Bus allowlist of the agent is not a security boundary while the terminal channel exists; the passkey step shows that the password was correct; dev_user and allow_user_agents give an agent of each local user a link, and are only for development; the CSP permits inline style elements (ADR-027); the session agent trusts /run/farglass by its ownership. A process of the user cannot replace the agent (F057). Each change that removes the cause of one of these risks supersedes this decision.

## Consequences
The lock screen protects against a person at the browser, not against programs of the user. The documentation must not call the D-Bus allowlist a protection. A rate limit, not the second factor, protects the password. farglassd in production refuses an agent of the user with no worker; the end-to-end tests use --allow-user-agents or --dev-user.
