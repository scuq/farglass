---
id: ADR-016
title: TLS certificate: files of the administrator, or a self-signed certificate for 30 days
status: accepted
date: 2026-11-19
tags: [tls, security]
superseded_by:
files: [crates/farglassd/src/tls.rs, packaging/farglassd.conf]
---

## Context
The browser needs TLS for the shell. A new host has no certificate. An administrator who has a certificate must be able to use it with little work.

## Decision
- With cert and key in the configuration, in the command line, or in the environment, farglassd uses those PEM files. It loads changed files again in less than one hour, and immediately after SIGHUP. If the new files are not valid, the old certificate stays active.
- With no cert and no key, farglassd makes a self-signed certificate in state_dir/tls. The key is ECDSA P-256. The certificate is valid for 30 days. farglassd makes a new one when the old one expires in less than one day, at the start and while it runs. A change of the names also makes a new one.
- farglassd writes the SHA-256 fingerprint of the certificate to the log.
- The TLS library is rustls 0.23 with aws-lc-rs. TLS 1.3 and TLS 1.2 are active.

## Consequences
A host works with no TLS configuration. The user must accept the self-signed certificate in the browser again each 30 days. WebTransport accepts a pinned self-signed certificate only if it is valid for 14 days or less. Thus F023 needs a second, shorter certificate or a different plan. All later listeners use CertStore.
