---
id: ADR-042
title: Security checks fail closed under host faults
status: accepted
date: 2026-11-20
tags: [security]
superseded_by:
files: []
---

## Context
The review of F035 found a passkey check that turned off when the passkey file could not be read. A host can run out of memory, disk space, or file descriptors, kill a process with the OOM killer, or give a read error. The security checks of F051 to F059 add more state on disk and in memory: blocks, device cookies, the PIN hash, the group, the passkeys.

## Decision
Each security check fails closed. A fault never loosens a check of a credential, a block, a limit, or a membership:
- A read error, a parse error, or a missing value of security state refuses the login or the action. Only a defined absence turns a check off, for example a missing passkey file (NotFound) or no PIN in the configuration
- A write error of security state refuses the action that needs the write, for example a recovery code that cannot be marked as used, or a block that cannot be stored
- No change overwrites a security file that could not be read
- A full memory table of the limiter removes old free entries, never a block that is still active, and refuses a new attempt if it has no place
- A panic, a dead task, a closed channel, or a timeout in a check gives a refusal, never an acceptance
- A process that restarts after an OOM kill starts with the state on disk, or with all logins refused until it can read that state. It never starts with an empty block list as permission for a blocked user: the blocks in memory are lost, and the log tells the administrator
- Each refusal for a fault goes to the journal at level error with reason=fault and the cause

## Consequences
Each security feature has tests with injected faults: an unreadable file, a full disk (a write error), a parse error, and a dead task. A fault can stop logins: an available service is less important than a check that holds. The administrator sees each fault in the journal.
