---
id: ADR-022
title: State store in the agent, connect again, logout, and idle logout
status: accepted
date: 2026-11-19
tags: [session, shell]
superseded_by:
files: [crates/farglass-session/src/state.rs, crates/farglassd/src/presence.rs, crates/farglassd/src/login.rs, web/src/App.tsx, web/src/state.ts]
---

## Context
ADR-004 says that the session continues when the tab closes and ends after an idle time. The shell must get its state back after a reload or a break. Windows come with F011, and the scrollback of terminals with F014. A user can have more than one tab (ADR-017).

## Decision
- The agent has a state store: JSON values by key, 8 MiB in total, for the life of the agent. It continues when the link to farglassd breaks. A channel of kind state gives get and set. A get also subscribes the channel to the changes of the key from other channels.
- Each kind of shell state chooses its key. State of one tab uses a key with a tab id from sessionStorage. State of the desktop, for example the windows of F011, uses a shared key, thus each tab shows the same desktop.
- After a break, the shell connects again after 1, 2, 4, 8, and then each 15 seconds, at once after the online event, the visibility of the tab, or Connect now. The cookie gives the user, and the state store gives the state.
- DELETE /session logs out: farglassd removes each cookie session of the user, sends logged-out to each connection of the user, and asks farglass-login to end the web sessions. farglass-login ends only sessions with type web and the PAM service farglass-session.
- farglassd counts the browser connections of each user. A user with an agent and no connection for idle_logout_minutes (default 480, 0 turns it off) is logged out in the same way. An examination runs each minute.

## Consequences
F011 keeps the window list in the state store. F014 adds the reattach of terminal channels with their scrollback. A restart of farglassd ends all cookie sessions (ADR-021), thus the shell then shows the login screen, but the logind session and the state store continue. A user logs out of all tabs and browsers at once.
