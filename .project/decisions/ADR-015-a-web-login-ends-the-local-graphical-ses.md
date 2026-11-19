---
id: ADR-015
title: A web login ends the local graphical session of the same user
status: accepted
date: 2026-11-19
tags: [session, login]
superseded_by:
files: []
---

## Context
farglass is for remote use only. No local user logs in through farglass. But a user who logs in through farglass can have a graphical session on the screen of the host at the same time, for example GNOME. The two sessions share one systemd user manager and one D-Bus session bus. Apps that permit one instance for each user, the keyring, and the portals then conflict. Many users must be able to work on one host at the same time.

## Decision
One user has one graphical session at a time.
- After the PAM login succeeds, farglass-login asks logind for the sessions of that user. A session is local and graphical if its class is user and its type is x11, wayland, or mir.
- If such a session exists, the login screen tells the user that the local session will end and that work that is not saved will be lost. The user must confirm.
- After the confirmation, farglass-login ends the local graphical sessions through logind and then starts the web session.
- If the user does not confirm, no web session starts, and the local session continues.
- Text sessions of the user (tty, ssh) continue.
- Sessions of other users are never changed.
- A second web login of the same user attaches to the existing web session (ADR-004).
- Different users have web sessions at the same time. farglassd serves all of them. Each user has one agent.

## Consequences
farglass-login needs the permission to end sessions through logind, which it has as root. The login protocol needs a confirm step between the PAM result and the session start (F007, F008). farglassd keeps its state for each user and for each connection, and has no state that assumes one user (F004). farglass does not control the display manager of the host. Thus a local login during a web session is possible. The confirm step is an addition to the request, to prevent a loss of data.
