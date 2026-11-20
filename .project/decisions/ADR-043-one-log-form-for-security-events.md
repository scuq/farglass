---
id: ADR-043
title: One log form for security events
status: accepted
date: 2026-11-20
tags: [security]
superseded_by:
files: []
---

## Context
F051 to F059 add many security events: refusals, blocks, timeouts, PIN results, group refusals, passkey changes, lock and logout. The administrator and fail2ban must find them in the journal of farglassd and farglass-login.

## Decision
Each security event goes through one tracing target, farglass::security, at level info for a success and warn for a refusal (error for a fault, ADR-042). The message has a fixed form: an event name, then key=value fields in a fixed order: event=NAME result=accepted|refused reason=REASON addr=ADDRESS user=USERNAME|- uid=UID|-. Example: event=login result=refused reason=address-cap addr=203.0.113.7 user=- uid=-. Values have no spaces. A username from the browser goes into the log only after a check of its characters, or as - . No password, PIN, recovery code, cookie, or ticket goes into the log.

## Consequences
One journalctl filter shows all security events: journalctl -u farglassd -u farglass-login | grep farglass::security. One fail2ban filter matches all refusals by addr. A test checks the form of each event name.
