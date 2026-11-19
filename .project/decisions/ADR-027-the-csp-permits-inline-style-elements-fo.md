---
id: ADR-027
title: The CSP permits inline style elements for the terminal
status: accepted
date: 2026-11-19
tags: [security, terminal]
superseded_by:
files: [crates/farglassd/src/http.rs]
---

## Context
ADR-021 gave a CSP with only 'self'. xterm.js makes inline <style> elements for each terminal: for the scrollbar colors, and in its DOM renderer for the rows, with class names unique to each terminal. A hash cannot permit content that changes for each terminal. Without WebGL2, xterm.js uses the DOM renderer, and a blocked style breaks it.

## Decision
The CSP adds style-src-elem 'self' 'unsafe-inline'. script-src stays 'self', with no inline script and no eval. style-src stays 'self', thus style attributes in markup stay blocked. Changes of style through the CSSOM were never blocked. The test of the security headers checks each of these directives. This decision changes only the style rule of ADR-021.

## Consequences
An injection of markup could also add CSS. The shell inserts no markup from a different origin: Solid writes text with textContent. The terminal works with the DOM renderer on clients with no WebGL2. A later terminal engine with no inline styles lets this rule go back to 'self'.
