---
id: ADR-046
title: farglassctl installs signed release tarballs
status: accepted
date: 2026-11-21
tags: [packaging]
superseded_by:
files: []
---

## Context
farglass must install and update on Debian, Ubuntu, Red Hat Enterprise Linux, Fedora, and Rocky Linux, on amd64 and arm64. install-local.sh works only from a source tree. F033 planned a .deb for Debian 13 only. chalk uses a static chalkctl binary and cosign keyless signatures.

## Decision
One static binary, farglassctl, installs and manages farglass. A GitHub release has, for each architecture, farglassctl and one tarball with farglassd, farglass-login, farglass-session, the web files, the extension, the systemd units, the PAM files, and the default configuration. A file SHA256SUMS lists them. The release workflow signs SHA256SUMS with cosign keyless; farglassctl accepts only the identity of .github/workflows/release.yml of scuq/farglass at a v tag, and refuses each file whose hash is not in the signed list. farglassctl installs the runtime packages with apt or dnf, and installs cosign if it is missing. The host programs are built on the oldest supported glibc (Rocky Linux 9); farglassctl is a static musl binary. Supported: Debian 12 and 13, Ubuntu 22.04 and 24.04, RHEL, Rocky, and AlmaLinux 9 and 10, Fedora (the two current releases). F033 is dropped.

## Consequences
No .deb or .rpm to maintain. farglassctl owns the files under /usr/libexec/farglass and /usr/share/farglass, and keeps /etc/farglass and /var/lib/farglass through updates. A release without a valid signature cannot install. install-local.sh stays for development.
