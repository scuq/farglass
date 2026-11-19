---
id: F034
title: Passkeys as a second factor
status: planned
phase: P06
tags: []
decisions: [ADR-004]
files: [crates/farglassd/, web/src/login/]
---

## Goal
A user can add a passkey and must use it after the PAM login.

## Requirements
- WebAuthn registration and login
- Storage for each user on the host
- A recovery path for a lost passkey

## Acceptance
