---
id: F009
title: Session cookie, Origin check, and Content Security Policy
status: done
phase: P02
tags: []
decisions: [ADR-003, ADR-019, ADR-021]
files: [crates/farglassd/src/cookie.rs, crates/farglassd/src/origin.rs, crates/farglassd/src/http.rs, crates/farglassd/src/ws.rs, crates/farglassd/src/login.rs, web/src/session.ts, web/src/App.tsx, spikes/e2e/login.mjs, docs/protocol.md, packaging/farglassd.conf]
---

## Goal
Only the browser that logged in can use the session.

## Requirements
- Cookie with HttpOnly, Secure, and SameSite=Strict
- Refuse a WebSocket connection with a different Origin
- Content Security Policy with no inline script, COOP and COEP headers

## Acceptance
- After a login, POST /session redeems a one-time ticket and sets __Host-farglass with HttpOnly, Secure, SameSite=Strict, and Path=/. Chromium confirms the attributes, and document.cookie does not show the cookie. A reload and a new tab skip the login. A browser with no cookie gets the login screen.
- A WebSocket with a different Origin or with no Origin gets 403. In dev_user mode, a host that is not a loopback name is refused. Tests confirm each case, also for POST /session.
- Each response has a CSP with 'self' only and no inline script or eval, COOP same-origin, and COEP require-corp. The shell in Chromium caused no CSP violation.
- HSTS is sent only with a certificate of the administrator. Tests confirm the two cases.
