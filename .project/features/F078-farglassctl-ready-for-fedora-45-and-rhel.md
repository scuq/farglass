---
id: F078
title: farglassctl ready for Fedora 45 and RHEL 10
status: done
phase: P06
tags: [packaging]
decisions: []
files: [crates/farglassctl/, README.md]
---

## Goal
farglassctl install, update, uninstall, and the management commands work on Fedora 45 and RHEL 10, tested in containers.

## Requirements
- The distribution table accepts Fedora 45 and RHEL 10, with tests
- install reports success only after farglassd answers on its port; doctor waits for the port a short time
- On RHEL, a failed dnf install gives the hint to register the system for the AppStream repository
- doctor checks that systemd-logind is available
- uninstall --purge prints no error when userdel removed the group farglass
- status aligns its columns
- Test in systemd containers of fedora:45 and ubi10 (with CentOS Stream 10 repositories): install, login in a browser, user, session, logs, update, doctor, uninstall --purge

## Acceptance
Tested in systemd containers on arm64 with a local test release (--from, --allow-unsigned): fedora:45, and ubi10 (ID=rhel 10.3) with CentOS Stream 10 BaseOS and AppStream. On both: install with the dnf packages, units active, doctor all OK, HTTPS 200, a browser login as alice shows the desktop with a logind session of type web, user add/remove/disable/enable, session list/logout, logs --security, update, uninstall --purge removes all. Fixes: install waits up to 15 s for the port before it reports success, doctor waits 5 s; doctor checks systemd-logind (ubi-init masks it); on RHEL a failed dnf install gives the subscription-manager hint (UBI repos have no pipewire, wireplumber, portal); TryRun steps hide stderr (groupdel after userdel, systemctl stop of a socket unit); status aligns its columns. Tests for Fedora 45, RHEL 10.3, AlmaLinux 10.1, and the hint. Not tested: amd64, SELinux enforcing (containers have none), a signed release, pin set.
