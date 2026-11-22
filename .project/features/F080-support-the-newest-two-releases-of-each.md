---
id: F080
title: Support the newest two releases of each distribution
status: done
phase:
tags: []
decisions: []
files: [crates/farglassctl/src/os.rs, README.md, CHANGELOG.md, .github/workflows/release.yml, .claude/skills/release/SKILL.md]
---

## Goal
farglass supports the newest release and the release before it of each distribution: Ubuntu LTS 26.04 and 24.04, Debian 13 and 12, RHEL, Rocky Linux, and AlmaLinux 10 and 9, Fedora 45 and 44.

## Requirements
- farglassctl refuses Ubuntu before 24.04 and Fedora before 44; Debian 12 and RHEL 9 stay the oldest
- A release newer than the newest one is not refused
- The smoke test of the release workflow starts the programs on the oldest release of each: Debian 12, Ubuntu 24.04, Rocky Linux 9, Fedora 44
- README, changelog, and the error text name the same list
- The release skill checks the list against the rule before each release

## Acceptance
The tests of crates/farglassctl accept Debian 12 and 13, Ubuntu 24.04 and 26.04, RHEL, Rocky Linux, and AlmaLinux 9 and 10, and Fedora 44 and 45, and refuse Debian 11, Ubuntu 22.04, Rocky Linux 8, and Fedora 43. The smoke test of release.yml names debian:12, ubuntu:24.04, rockylinux:9, and fedora:44; it did not run yet, the next release runs it. README, changelog, error text, and the release skill name the same list.
