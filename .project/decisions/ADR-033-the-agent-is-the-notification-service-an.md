---
id: ADR-033
title: The agent is the notification service and the shell times the notifications
status: accepted
date: 2026-11-19
tags: [notify, dbus]
superseded_by:
files: []
---

## Context
Programs send desktop notifications to org.freedesktop.Notifications on the session bus. The user bus is per user, thus a local GNOME session and the web session of the same user share it. A notification can come while no tab is open, and the time to show is wrong if it runs while nobody looks.

## Decision
The agent owns org.freedesktop.Notifications and keeps each notification until it closes, at most 100. It asks for the name with no DO_NOT_QUEUE and no REPLACE_EXISTING: it waits in the queue of the bus and never takes the name from a local desktop. The shell starts the time to show when the notification is on the screen, and pauses it on hover, focus, and a hidden tab. The shell sends expire, dismiss, or action, and the agent sends the signals. The body is plain text.

## Consequences
A notification waits for the first tab. Each tab shows each notification, and an expire in one tab closes it in all tabs. A later notification center (history) can read the store of the agent. A program that sends markup shows the tags as text.
