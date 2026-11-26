---
id: ADR-059
title: The agent sets the GTK and KDE theme of the user during a web session and restores it
status: accepted
date: 2026-11-26
tags: [theme, session]
superseded_by:
files: []
---

## Context
The apps of a web session read their dark or light theme from the settings of the user: GTK and the desktop portal from GSettings (org.gnome.desktop.interface color-scheme, and gtk-theme for GTK3), KDE apps from ~/.config/kdeglobals. The web session and the local desktop of the user share these settings. A web login ends the local graphical session (ADR-015), thus the two never run at the same time. The shell has its own theme: LCARS, system, light, or dark.

## Decision
While a web session runs, the agent sets the theme of the user to the one of the shell, and restores the values of the user when the session ends.

- The shell sends the resolved choice on a theme channel: dark for Dark and LCARS, the setting of the browser for System.
- Before its first change, the agent saves the values of the user in ~/.local/state/farglass/theme-restore.json. A file that is there at the start of the agent comes from a crash: its values stay the original ones.
- GSettings: color-scheme prefer-dark or prefer-light (default where prefer-light does not exist). gtk-theme Adwaita-dark or Adwaita, only if the user has an Adwaita theme.
- KDE: the colors of Breeze Dark or Breeze Light in kdeglobals, and the notifyChange signal of KGlobalSettings. Only if the Breeze color schemes are installed.
- On SIGTERM, the agent restores the values and deletes the file.
- The agent does this only in a web session: where the user manager has a display of a local desktop, it changes nothing. FARGLASS_FOLLOW_THEME=1 or 0 overrides it.

## Consequences
The apps of the web session follow the shell, also apps that run when the theme changes. The local desktop of the user looks as before after the web session. A change of the theme in the settings of an app during the web session goes back at the end. After a crash of the agent, the values of the user come back at the end of the next web session. The agent needs gsettings and gdbus (libglib2.0-bin), which a GNOME or GTK host has.
