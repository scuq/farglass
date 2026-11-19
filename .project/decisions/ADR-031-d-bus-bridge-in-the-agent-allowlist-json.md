---
id: ADR-031
title: D-Bus bridge in the agent: allowlist, JSON form, and one task for each call
status: accepted
date: 2026-11-19
tags: [dbus, protocol]
superseded_by:
files: [crates/farglass-session/src/dbus.rs, crates/farglass-proto/src/message.rs, web/src/dbus.ts]
---

## Context
The settings (F022), the polkit agent (F019), and notifications (F020) need D-Bus from the shell. ADR-009 says the agent calls system services through D-Bus as the user. zbus 6 merged zvariant into zbus and has no public call that returns the raw reply with a flag for interactive authentication.

## Decision
- A channel of kind dbus has one bus, system or session. DbusRequest has call, subscribe, and unsubscribe, with an id from the shell. DbusEvent has reply, error, and signal.
- The agent permits only the bus names of DbusOptions: as the destination of a call and as the sender of a subscription. An entry is a name or a prefix with .*. The default system list is org.freedesktop.DBus, hostname1, locale1, login1, NetworkManager, timedate1, and UPower. The default session list is org.freedesktop.DBus.
- JSON form: numbers, strings, and booleans as they are, a 64-bit number also as a string, arrays and structures as arrays, dictionaries as objects, a variant as {type, value}. The shell gives the signature of the arguments, thus the conversion needs no guess.
- The agent opens one connection to each bus at its first use. Each call runs in its own task. It sends the message with ALLOW_INTERACTIVE_AUTH if the shell asks for it, and takes the reply with the same serial from a message stream that it opened before the send. An error reply gives its own D-Bus name and message, not a zbus mapping.
- A subscription is a match rule with the sender. Its task sends each signal. The subscriptions of a channel end with the channel.
- web/src/dbus.ts is the client of the shell: call, getProperty, getAllProperties, setProperty, and subscribe. AppContext gives the connection, thus an app can open a DbusClient.

## Consequences
A new system service for the settings needs an entry in the allowlist. The allowlist protects against a script in the shell that is not ours, but the terminal gives the user full access anyway. The interactive flag makes polkit ask the authentication agent of the session, which is F019.
