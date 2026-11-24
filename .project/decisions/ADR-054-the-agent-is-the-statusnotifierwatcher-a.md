---
id: ADR-054
title: The agent is the StatusNotifierWatcher and host
status: accepted
date: 2026-11-24
tags: [tray, dbus]
superseded_by:
files: []
---

## Context
Programs such as Bitwarden and Seafile close their window and stay in a tray. They look for org.kde.StatusNotifierWatcher on the session bus and register a StatusNotifierItem there. With no watcher they show no icon, and the user has no way back to the window. The user bus is per user, thus a local desktop and the web session of the same user share it.

## Decision
The agent owns org.kde.StatusNotifierWatcher and is also the host: it reads each item and sends it to the shell on the tray channel, and the panel shows one button for each item. It asks for the name with no DO_NOT_QUEUE and no REPLACE_EXISTING, as for notifications (ADR-033). The agent gives each item a number, and bus names do not go to the browser. The agent sends icons as PNG or SVG in the event. There is no XEmbed tray and no key in farglassd.conf. Each value from an item has a limit, because each program on the bus can register an item.

## Consequences
A program that only has an XEmbed tray shows no icon. While a local desktop owns the name, the tray of the web session is empty. A program that started before the agent shows its icon only when it registers again. The compositor has no xdg-activation: a click on an item shows a window only because the program makes a new window. The menus of the items (com.canonical.dbusmenu) need a menu with submenus and check marks in the shell.
