---
id: F067
title: farglassctl install, update, and uninstall
status: done
phase: P06
tags: [packaging]
decisions: [ADR-042, ADR-048]
files: [crates/farglassctl/, packaging/pam.d/rhel/, packaging/release.sh, .github/workflows/release.yml, crates/farglassd/tests/server.rs, Cargo.toml, Cargo.lock, README.md]
---

## Goal
farglassctl install sets up a complete farglass host from a signed release, and update replaces it with a newer one.

## Requirements
- Distribution from /etc/os-release: Debian, Ubuntu, RHEL, Rocky, AlmaLinux, Fedora; refuse others with the list
- Runtime libraries and session programs with apt or dnf
- cosign: the static release binary for the architecture, accepted only with the SHA-256 in farglassctl (ADR-047)
- Download with rustls from the GitHub release, or --from DIR for a local file; verify the cosign signature of SHA256SUMS and each hash; fail closed
- Install the files of install-local.sh, the group farglass, the user farglass, and the group farglass-users; keep /etc/farglass and /var/lib/farglass
- SELinux: restorecon on the installed paths when SELinux is on; firewalld: a hint or --open-firewall
- update: same checks, a stop and start of the units, and a list of web sessions with the old agent
- uninstall [--purge]
- --user USER on install adds the user to farglass-users
- Tests: the distribution table, the verification, and the file plan

## Acceptance
farglassctl install/update/uninstall build a plan of steps (os.rs, install.rs, plan.rs) from the distribution of /etc/os-release and the facts of the host, and apply it or print it with --dry-run. release.rs gets the newest or a given release (ureq with rustls) or --from DIR, downloads cosign v3.1.4 and runs it only with the embedded SHA-256, verifies the bundle of SHA256SUMS against release.yml at a v tag, checks the SHA-256 of the tarball, and extracts it in Rust with no tar program. RHEL-family PAM files with password-auth; restorecon on SELinux; firewalld note or --open-firewall; the config only when missing; usermod for --user. The farglassd test clients name the aws-lc-rs provider, because the workspace has ring too. Tests: 16 unit tests (distribution table, file steps, hash checks, unsafe tarballs, plans for Debian and Rocky, update, uninstall, purge). A dry run on this Debian 13 arm64 host from a locally built release lists the expected 22 steps. Not tested: a real install as root, the signed path against a real release, and the static musl build with ring (needs musl-gcc, set in release.sh for CI).
