---
id: F053
title: A passkey change needs a fresh passkey
status: done
phase: P06
tags: [security]
decisions: [ADR-042, ADR-043]
files: [crates/farglassd/src/passkeys.rs, crates/farglass-proto/src/message.rs, web/src/apps/settings/pages/Passkeys.tsx, web/src/apps/settings/settings.css, spikes/e2e/passkey.mjs, README.md, docs/threats.md]
---

## Goal
A person at an unlocked tab or with a stolen cookie cannot remove the second factor.

## Requirements
- Remove, new recovery codes, and the registration ask for a passkey of the user first, or a recovery code
- Review finding 5 in docs/threats.md

## Acceptance
A user with a passkey, or with a passkey file that cannot be read, gets PasskeyEvent::Confirm before Register, Remove, and NewCodes. The shell answers with Proof (navigator.credentials.get) or ProofCode; farglassd verifies with the store, then does the change. The proof is valid for 5 minutes on that channel; three wrong proofs close it with AccessDenied. A user with no passkey adds the first one with no proof. Each proof and change goes to farglass::security; file faults log as faults. Tests: two PasskeysTask tests (no proof for the first passkey; damaged file needs a proof that fails and nothing changes); e2e passkey: new codes need a fresh passkey, the same window needs no new proof, a new window needs it, and a wrong code changes nothing.
