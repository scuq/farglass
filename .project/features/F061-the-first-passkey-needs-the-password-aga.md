---
id: F061
title: The first passkey needs the password again
status: planned
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: []
---

## Goal
A person at an unlocked tab or with a stolen cookie cannot add a passkey to an account with no passkey and so lock out the user.

## Requirements
- The first registration asks for the password of the user through farglass-login, with the limits of F051
- Review finding 15 in docs/threats.md

## Acceptance
