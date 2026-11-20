---
id: F066
title: Release workflow for amd64 and arm64
status: planned
phase: P06
tags: [packaging]
decisions: [ADR-046]
files: []
---

## Goal
A v tag builds, tests, signs, and publishes farglassctl and the farglass tarball for amd64 and arm64.

## Requirements
- .github/workflows/release.yml on v tags and on workflow_dispatch at a tag, with the tag guard of chalk
- Host programs built in a Rocky Linux 9 container on native amd64 and arm64 runners, the web files once
- farglassctl built static with musl
- SHA256SUMS signed with cosign sign-blob keyless
- A smoke test runs the programs on Debian 12, Ubuntu 22.04, and Rocky Linux 9 images
- .github/workflows/ci.yml runs the tests on each push

## Acceptance
