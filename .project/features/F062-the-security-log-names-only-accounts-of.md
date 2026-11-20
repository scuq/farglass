---
id: F062
title: The security log names only accounts of the host
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglass-login/src/server.rs, README.md, docs/threats.md]
---

## Goal
A password typed in the username field does not go into the journal.

## Requirements
- A failed login logs the username only if the host has that account, else user=unknown
- The block of the user still counts the typed name
- Review finding 16 in docs/threats.md

## Acceptance
log_event of farglass-login writes the username only if nix finds the account on the host, else user=?; a read error of the user database also gives user=?. The limiter still counts the typed name. Test: the_log_names_an_account_of_the_host_and_hides_other_text.
