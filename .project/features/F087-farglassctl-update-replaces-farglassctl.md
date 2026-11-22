---
id: F087
title: farglassctl update replaces farglassctl
status: done
phase:
tags: [farglassctl, release]
decisions: []
files: [crates/farglassctl/src/release.rs, crates/farglassctl/src/install.rs, crates/farglassctl/src/main.rs, README.md, CHANGELOG.md]
---

## Goal

## Requirements

## Acceptance
farglassctl update downloads farglassctl-linux-ARCH with the release, checks its SHA-256 against the signed SHA256SUMS, and copies it to /usr/local/sbin/farglassctl. A release dir with no farglassctl keeps the old behavior. Unit tests cover the hash check and the plan step.
