---
id: F069
title: Install test on each supported distribution
status: planned
phase: P06
tags: [packaging]
decisions: [ADR-048]
files: []
---

## Goal
Each release proves that farglassctl installs on each supported distribution and architecture.

## Requirements
- CI runs farglassctl install --from dist in systemd containers of Debian 12 and 13, Ubuntu 22.04 and 24.04, Rocky Linux 9 and 10, Fedora, on amd64 and arm64
- The test checks the units, the HTTPS answer, and a login with a PAM test stack
- doctor passes after the install

## Acceptance
