---
id: F035
title: Security review
status: done
phase: P06
tags: []
decisions: [ADR-041]
files: [docs/threats.md, crates/farglassd/src/passkeys.rs]
---

## Goal
A review of the login, the session start, the channel allowlists, and the web headers.

## Requirements
- A threat list in docs/
- Each finding is a feature or a decision

## Acceptance
docs/threats.md lists attackers, entry points with their controls, and 12 findings. Finding 1 (high: the passkey second factor failed open on a read error) is fixed with tests: only NotFound means no passkey, and no change overwrites an unreadable file. Findings 2-10 and 12 are features F051-F056; the by-design items are ADR-041.
