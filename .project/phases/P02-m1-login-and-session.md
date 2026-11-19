---
id: P02
title: M1 login and session
status: done
---

## Goal
A user logs in from the client browser and gets a real session on the host.

## Exit criteria
A PAM login with a password and a one-time code works from the login screen. loginctl shows a session of type web. The session continues when the tab closes and ends on logout or idle timeout.
