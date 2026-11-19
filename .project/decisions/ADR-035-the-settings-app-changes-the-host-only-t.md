---
id: ADR-035
title: The settings app changes the host only through polkit, with a confirm step for actions that end sessions
status: accepted
date: 2026-11-19
tags: [settings, security]
superseded_by:
files: []
---

## Context
The settings app runs in a remote browser. A change of the network, a restart, or the end of a session can stop the connection that shows the app, and the user can be far from the host.

## Decision
Each page calls a system service on the dbus channel with interactive authorization, thus polkit asks in the shell when the service needs it. Restart, Power off, and the end of a session need a confirm step that names the result, with the focus on Cancel. The network page only shows NetworkManager and marks the device of the current connection. The appearance is a key of the session in the state store (watchShared and setShared), which a closed window does not remove.

## Consequences
A user cannot lock themselves out through the network page. A later network editor must warn before a change to the device of the current connection. Apps that keep a value for the session use setShared, and the watchers of the same tab get it at once.
