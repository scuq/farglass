---
id: ADR-047
title: farglassctl installs signed release tarballs with few dependencies
status: accepted
date: 2026-11-21
tags: [packaging]
superseded_by:
files: []
---

## Context
ADR-046 chose release tarballs, cosign keyless signatures, and a cosign from the package manager. The administrator wants each binary to run on each supported host with as few outside dependencies as possible: static where possible.

## Decision
As ADR-046, with these changes:
- farglassctl is fully static (musl). It uses rustls for HTTPS and runs no curl, wget, or OpenSSL.
- farglassctl does not take cosign from apt or dnf. It downloads the static cosign release binary of the sigstore project for its architecture and accepts it only with the SHA-256 that farglassctl embeds. An update of cosign is an update of farglassctl.
- farglassd, farglass-login, and farglass-session link libgcc and libstdc++ statically. They link dynamically only with libc, libpam (PAM loads its modules at run time), libpipewire (the sound server of the session), and libxkbcommon. They are built on the oldest supported glibc (Rocky Linux 9, glibc 2.34).
- farglassctl installs with apt or dnf only these runtime libraries and the session programs (PipeWire, the portal), and checks them with doctor.

## Consequences
A host with no cosign, curl, or OpenSSL can install farglass. The list of dynamic libraries is a test of the release workflow: a new library fails the build until this decision changes.
