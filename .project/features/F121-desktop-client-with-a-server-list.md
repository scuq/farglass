---
id: F121
title: Desktop client with a server list
status: done
phase: P06
tags: [client]
decisions: [ADR-060]
files: [client/, crates/farglassd/src/tls.rs, packaging/farglassd.conf, .github/workflows/ci.yml, .github/workflows/release.yml, .claude/skills/release/SKILL.md, .gitignore, README.md, CHANGELOG.md]
---

## Goal
A Linux user starts the farglass desktop client, selects a server from a list, and gets the shell in a window with no browser around it.

## Requirements
- client/ is an Electron app with a server list at the start: add, edit, remove, connect. The list is a private JSON file in the user data folder.
- The shell window has the sandbox, no preload, its own session for each server, permissions only for the origin of the server, and external links in the desktop browser.
- Trust on first use of the public key of a self-signed certificate (ADR-060). A changed key stops the connection.
- farglassd keeps the key of its self-signed certificate on renewal.
- ci.yml tests the client and builds it unpacked. release.yml builds an AppImage and a deb for amd64 and arm64, and SHA256SUMS lists them.
- The release skill sets the version in client/package.json.

## Acceptance
client/ builds with Electron 45.5.0: 11 vitest tests pass (address rules, server store, public-key pin equal to the openssl command on the host, same pin after a new certificate with one key). npm run dist built the AppImage and the deb for arm64. The built client against the local farglassd: the server list shows, a pinned server opens the login screen of the shell, the shell window has no window.farglass and no require, and a server with a different key is refused with a message in the list. farglassd keeps the key of its self-signed certificate on renewal (test new_certificate_keeps_the_key). ci.yml has a client job, and release.yml builds the client for amd64 and arm64 and lists it in SHA256SUMS. The test mic an_app_records_the_packets_from_the_source was skipped: it fails on this VM at HEAD too, because PipeWire records no tone.
