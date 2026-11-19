---
id: ADR-034
title: A lock ends each connection of the user and only a login unlocks
status: accepted
date: 2026-11-19
tags: [lock, security]
superseded_by:
files: []
---

## Context
A locked session must show nothing of the desktop and must give the browser no access to the agent. The shell runs in the browser, thus a lock that only covers the desktop with a dialog keeps the windows, the terminals, and the channels in the page.

## Decision
farglassd keeps the lock state of each user in memory. A lock sends locked to each connection of the user and closes it, thus each channel ends and the shell drops the desktop. A new connection of a locked user gets no user and the field locked in its hello message, and its login channel logs in only that user. A successful login unlocks the session, and each other tab on the lock screen gets unlocked and connects again. The shell locks with a connection message, farglassd locks after idle_lock_minutes with no keyboard or pointer activity (reported at most each 30 s), and the agent relays Lock and Unlock of logind and sets LockedHint. On the lock screen, DELETE /session removes only the cookie.

## Consequences
The lock state is lost when farglassd restarts, but the cookies are also lost, thus each tab must log in. An unlock request of logind is accepted: the owner of a session can already unlock it with loginctl. Each app must keep its state in the agent, because a lock ends the page side of each app. A later screen saver or a blur of the desktop is not possible with this design.
