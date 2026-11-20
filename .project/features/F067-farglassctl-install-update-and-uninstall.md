---
id: F067
title: farglassctl install, update, and uninstall
status: planned
phase: P06
tags: [packaging]
decisions: [ADR-042, ADR-046]
files: []
---

## Goal
farglassctl install sets up a complete farglass host from a signed release, and update replaces it with a newer one.

## Requirements
- Distribution from /etc/os-release: Debian, Ubuntu, RHEL, Rocky, AlmaLinux, Fedora; refuse others with the list
- Runtime packages with apt or dnf; cosign if missing
- Download from the GitHub release, or --from DIR for a local file; verify the cosign signature of SHA256SUMS and each hash; fail closed
- Install the files of install-local.sh, the group farglass, the user farglass, and the group farglass-users; keep /etc/farglass and /var/lib/farglass
- SELinux: restorecon on the installed paths when SELinux is on; firewalld: a hint or --open-firewall
- update: same checks, a stop and start of the units, and a list of web sessions with the old agent
- uninstall [--purge]
- --user USER on install adds the user to farglass-users
- Tests: the distribution table, the verification with a test key path, and the file plan

## Acceptance
