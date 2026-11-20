---
id: F066
title: Release workflow for amd64 and arm64
status: done
phase: P06
tags: [packaging]
decisions: [ADR-048]
files: [.github/workflows/ci.yml, .github/workflows/release.yml, packaging/release.sh, crates/farglassctl/src/main.rs, README.md]
---

## Goal
A v tag builds, tests, signs, and publishes farglassctl and the farglass tarball for amd64 and arm64.

## Requirements
- .github/workflows/release.yml on v tags and on workflow_dispatch at a tag, with the tag guard of chalk
- Host programs built in a Rocky Linux 9 container on native amd64 and arm64 runners, with libgcc and libstdc++ static; the web files once
- A check of the dynamic libraries of each program: only libc, libpam, libpipewire, and libxkbcommon (ADR-047)
- farglassctl built static with musl; a check that it has no dynamic library
- SHA256SUMS signed with cosign sign-blob keyless
- A smoke test runs the programs on Debian 12, Ubuntu 22.04, and Rocky Linux 9 images
- .github/workflows/ci.yml runs the tests on each push

## Acceptance
release.yml: a tag guard; the shell once; the host programs in rockylinux:9 on ubuntu-24.04 and ubuntu-24.04-arm through packaging/release.sh host, which links libstdc++ static and fails on a library outside ADR-048; farglassctl static musl through release.sh ctl; a smoke test of ldd and --version on debian:12, ubuntu:22.04, and rockylinux:9 for each arch; SHA256SUMS signed with cosign sign-blob keyless into SHA256SUMS.sigstore.json; gh release create. ci.yml: fmt, clippy, tests, cargo audit, web tests, npm audit, build, static farglassctl. Local test on arm64 Debian 13: release.sh host wrote a 12.8 MB tarball with only permitted libraries, release.sh ctl a static farglassctl that prints the stamped version. The workflows parse as YAML; they did not run on GitHub yet.
