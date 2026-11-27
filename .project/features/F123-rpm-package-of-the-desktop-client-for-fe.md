---
id: F123
title: rpm package of the desktop client for Fedora and RHEL 10
status: done
phase: P06
tags: [client]
decisions: [ADR-060]
files: [client/electron-builder.yml, client/package.json, .github/workflows/release.yml, README.md, CHANGELOG.md]
---

## Goal
A user of Fedora or RHEL 10 installs the desktop client with dnf.

## Requirements
- electron-builder makes an rpm with the dependencies of Fedora 44 and RHEL 10, and _build_id_links none, so that it does not conflict with other Electron rpm packages.
- release.yml installs rpmbuild, renames x64, x86_64, and aarch64 to amd64 and arm64, and publishes the rpm in SHA256SUMS.
- The job client-smoke installs the deb on Debian 12 and Ubuntu 24.04 and the rpm on Fedora 44 and AlmaLinux 10, on amd64 and arm64, and checks with ldd that no library is missing. The release waits for it.
- The deb depends on libgbm1 and libasound2 or libasound2t64.

## Acceptance
electron-builder 26.18.0 built farglass-client-0.4.0-linux-aarch64.rpm and the deb in a node:26-trixie container with rpmbuild. dnf installed the rpm on fedora:44 and almalinux:10 (aarch64): rpm -q shows farglass-client-0.4.0-1, ldd of /opt/farglass/farglass-client finds all libraries, /usr/bin/farglass-client and the desktop file exist. apt installed the deb on debian:12 and ubuntu:24.04 with no missing library. Before the fix of the deb dependencies, ldd on debian:12 showed libasound.so.2 not found. The amd64 packages and the client-smoke job run first at the next release tag. The mic test was skipped as in F121.
