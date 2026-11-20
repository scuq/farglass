---
id: F056
title: Hardening of farglass-login
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [packaging/systemd/farglass-login.service, packaging/install-local.sh, crates/farglass-login/src/server.rs, README.md, docs/threats.md]
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
farglass-login.service gets CapabilityBoundingSet=CAP_AUDIT_WRITE CAP_CHOWN CAP_DAC_OVERRIDE CAP_DAC_READ_SEARCH CAP_FOWNER CAP_SETGID CAP_SETUID, no ambient capabilities, ProtectProc=invisible, UMask=0077, PrivateIPC=yes; systemd-analyze verify finds no error. farglass-login refuses a socket peer that is not root, the user farglass, or its own user, with a security log line (unit test peer_allowed). install-local.sh gives the repository path to runuser in FARGLASS_REPO with -w, not in the command text (sh -n passes). Not tested: a real root PAM login with the new capability set; run sudo spikes/session/check.sh after the install.
