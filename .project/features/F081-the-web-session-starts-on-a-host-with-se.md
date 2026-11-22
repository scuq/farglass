---
id: F081
title: The web session starts on a host with SELinux in enforcing mode
status: done
phase:
tags: [security, install]
decisions: []
files: [packaging/selinux/, packaging/release.sh, crates/farglassctl/src/install.rs, crates/farglass-login/src/host.rs, README.md, CHANGELOG.md]
---

## Goal
A user can log in on a host with SELinux in enforcing mode, for example RHEL with the default policy.

## Requirements
- A CIL policy module in packaging/selinux/ lets unconfined_service_t start a process in unconfined_t (the AVC of a RHEL host: denied transition, scontext unconfined_service_t, tcontext unconfined_t, path farglass-session)
- The release tarball contains the module
- farglassctl install and update load the module with semodule on a host with SELinux; uninstall removes it
- farglass-login stops the wait when the session worker stopped, and its error names where the cause is

## Acceptance
packaging/selinux/farglass.cil permits the transition from unconfined_service_t to unconfined_t in an optional block; the rule is the one that audit2allow made from the AVC of a RHEL host, and with that rule loaded a FreeIPA user logged in with SELinux in enforcing mode. release.sh puts packaging/selinux in the tarball. farglassctl install and update run semodule -X 200 -i on a host with SELinux, and uninstall runs semodule -X 200 -r farglass (tests of the install plan and the uninstall plan). farglass-login stops the wait when systemctl is-active fails for the unit of the session worker, and its error names the journal and ausearch. Not tested: the load of the CIL file with semodule on a host with SELinux, and the early stop on a real host.
