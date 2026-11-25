---
id: F111
title: Network check app: internet, DNS, and own ICMP and TCP checks
status: done
phase:
tags: [network, settings]
decisions: [ADR-055]
files: [crates/farglass-proto/src/message.rs, crates/farglass-proto/src/typescript.rs, web/src/proto/generated/messages.ts, crates/farglass-session/src/netcheck.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/bookmarks.rs, crates/farglass-session/src/lib.rs, crates/farglass-session/Cargo.toml, crates/farglass-session/tests/netcheck.rs, web/src/apps/netcheck/, web/src/apps/settings/pages/Network.tsx, web/src/apps/settings/Settings.tsx, web/src/apps/apps.test.tsx, web/src/menu/menu.test.tsx, web/src/panel/Panel.test.tsx, spikes/e2e/netcheck.mjs, spikes/e2e/package.json, README.md, CHANGELOG.md]
---

## Goal
A Network check app shows at a glance if the host reaches the internet, DNS works, and the connections that the user needs are available or blocked.

## Requirements
- HTTPS checks to 1.1.1.1 and google.com, also through https_proxy when it is set
- A DNS check with the system resolver and the name servers
- The user adds, changes, and removes ICMP and TCP checks; the administrator checks from /etc are read-only
- All checks run when the app opens, with short timeouts; Check again runs them again, and each row can run alone
- Settings > Network opens the app

## Acceptance
The Network check app (netcheck channel, agent module netcheck.rs) runs on open, with Check again and a Check button on each row: HTTPS to 1.1.1.1 and google.com with the CA file of the host, the same through an http:// https_proxy when set, a DNS lookup of google.com with the name servers of resolv.conf, and ICMP and TCP checks from ~/.config/farglass/netchecks.toml (edited in the app, comments kept) and read-only /etc/farglass/netchecks.toml. ICMP uses the kernel ping socket and falls back to ping. States: Available with ms, Failed with the cause (Refused tells a closed port), No answer after the timeout; a sticky summary shows Internet, DNS, and N of M available; a DNS failure gives an alert; results older than 5 min are dim with their age. Settings > Network has Check connections. Tests: 6 unit tests and tests/netcheck.rs over the link (an ignored test reached 1.1.1.1 and google.com from this host), 7 vitest tests, spikes/e2e/netcheck.mjs 16/16 in Chromium. Not tested: the proxy path against a real proxy, and the ping program fallback.
