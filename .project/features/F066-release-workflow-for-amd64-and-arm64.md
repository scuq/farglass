---
id: F066
title: Release workflow for amd64 and arm64
status: planned
phase: P06
tags: [packaging]
decisions: [ADR-047]
files: []
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
