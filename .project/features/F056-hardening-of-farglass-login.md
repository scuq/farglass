---
id: F056
title: Hardening of farglass-login
status: planned
phase: P06
tags: [security]
decisions: []
files: []
---

## Goal
A defect in farglass-login gives an attacker less than all of root.

## Requirements
- CapabilityBoundingSet with only the capabilities that PAM, logind, and the session start need
- ProtectProc=invisible, UMask=0077, PrivateIPC=yes in farglass-login.service
- farglass-login refuses a peer that is not root or the farglass user
- install-local.sh passes the repository path as an argument, not inside a shell string
- Review finding 9 and the install-local.sh item in docs/threats.md

## Acceptance
