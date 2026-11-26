---
id: ADR-060
title: The desktop client is Electron that loads the shell from the host and pins its certificate
status: accepted
date: 2026-11-27
tags: [client, security]
superseded_by:
files: [client/, crates/farglassd/src/tls.rs]
---

## Context
Users want a fixed list of farglass servers and a window with no browser around it. A self-signed certificate of farglassd shows a warning in each browser, and the user cannot tell a new certificate from an attack. farglassd makes a new self-signed certificate each 30 days.

## Decision
- The desktop client in client/ is Electron for Linux, as an AppImage and a deb package for amd64 and arm64, built by ci.yml and release.yml.
- The client does not contain the shell. A window loads the shell from farglassd, thus the shell has the version of the host.
- A window of the shell has the sandbox, context isolation, no preload, and one session partition for each server. It gets only the permissions media, clipboard, notifications, fullscreen, keyboardLock, and pointerLock, and only from the origin of its server. Other origins open in the browser of the desktop.
- If Chromium refuses the certificate, the client compares the SHA-256 of the public key (SPKI) with the pin of the server. The user accepts the pin at the first connection. A different key stops the connection.
- farglassd keeps the key of its self-signed certificate when it makes a new certificate.

## Consequences
A host with the self-signed certificate keeps one key for its life. To change the key, the administrator removes the key file and each user forgets the pin in the client. The video redirection extension does not work in the client, because Electron has no native messaging. The client has no menu and no browser shortcuts, thus Ctrl+W and Ctrl+R go to the shell. Windows and macOS builds need a code signature and are not part of this decision.
