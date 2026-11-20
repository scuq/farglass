---
id: ADR-041
title: Accepted risks of the security review
status: accepted
date: 2026-11-20
tags: [security]
superseded_by:
files: []
---

## Context
The security review of F035 (docs/threats.md) found items that come from the design, not from defects: the shell gives the user a terminal, the agent runs as the user, and the login asks for the password before the passkey.

## Decision
These risks stay: a process of the user can unlock the web lock screen or replace the agent, as with logind; the D-Bus allowlist of the agent is not a security boundary while the terminal channel exists; the passkey step shows that the password was correct; dev_user gives each local user a session with no login and is only for development; the CSP permits inline style elements (ADR-027); the session agent trusts /run/farglass by its ownership. Each change that removes the cause of one of these risks supersedes this decision.

## Consequences
The lock screen protects against a person at the browser, not against programs of the user. The documentation must not call the D-Bus allowlist a protection. A rate limit, not the second factor, protects the password.
