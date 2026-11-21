---
id: P06
title: M5 packages and protection
status: planned
---

## Goal
A host that an administrator can install and trust.

## Exit criteria
farglassctl installs farglass from a signed release tarball on each supported distribution, on amd64 and arm64 (ADR-046, F069). The repository has a license, a CHANGELOG, and a release tag. Passkeys work as a second factor. The security review has no open finding. The shell works in Firefox and Safari.
