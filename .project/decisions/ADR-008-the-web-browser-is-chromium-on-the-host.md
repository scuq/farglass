---
id: ADR-008
title: The web browser is Chromium on the host
status: accepted
date: 2026-11-19
tags: []
superseded_by:
files: []
---

## Context
The web platform does not permit a page of any site inside a farglass window at native speed. An iframe fails on most sites. Controlled Frame is in ChromeOS only. No usable browser engine exists in JavaScript or WebAssembly. A proxy that removes X-Frame-Options breaks logins and site isolation.

## Decision
The web browser app is Chromium on the host, shown through the surface pipeline. There is no browser app before that milestone. Until then, a link opens in a new tab of the client browser, with a label that it leaves the host.

## Consequences
The first usable desktop has no browser of its own. The browser is in the Linux session and uses the host network. If Controlled Frame becomes available on other platforms, examine it again.
