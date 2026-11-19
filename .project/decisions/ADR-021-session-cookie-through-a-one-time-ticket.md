---
id: ADR-021
title: Session cookie through a one-time ticket, Origin check, and security headers
status: accepted
date: 2026-11-19
tags: [security, session]
superseded_by:
files: [crates/farglassd/src/cookie.rs, crates/farglassd/src/origin.rs, crates/farglassd/src/http.rs, web/src/session.ts]
---

## Context
The login runs on the WebSocket (ADR-019), and a WebSocket message cannot set a cookie. A page of a different site can open a WebSocket to farglassd, and the browser sends the cookie with it. dev_user gives a user with no login on loopback, and DNS rebinding can point a different name at 127.0.0.1. HSTS makes a certificate error final in the browser.

## Decision
- After a login, farglassd puts a one-time ticket (32 random bytes, valid for 60 seconds) in the success event. POST /session redeems it and sets __Host-farglass with Path=/, Secure, HttpOnly, and SameSite=Strict, with no Max-Age.
- farglassd keeps the SHA-256 hash of each ticket and token in memory. A session is valid for 12 hours. DELETE /session removes it.
- The WebSocket handshake and the session endpoint need Origin equal to https:// and the host of the request, or an origin from allowed_origins. A missing Origin is refused. In dev_user mode, the host must be a loopback name.
- Each response has a CSP with only 'self' and no inline script, COOP same-origin, COEP require-corp, CORP same-origin, Referrer-Policy no-referrer, and X-Frame-Options DENY.
- HSTS only with a certificate of the administrator, never with a self-signed certificate.

## Consequences
A reload or a new tab needs no login. A restart of farglassd needs a new login, because the sessions are in memory. F010 adds the idle timeout and the logout, and ties the cookie session to the logind session. COEP require-corp means that each resource must come from this server, also later fonts and images. A Vite development setup needs allowed_origins.
