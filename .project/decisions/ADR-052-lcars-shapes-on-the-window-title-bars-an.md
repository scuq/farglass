---
id: ADR-052
title: LCARS shapes on the window title bars and the workspaces
status: accepted
date: 2026-11-22
tags: [web, ui]
superseded_by:
files: [web/src/theme/lcars.css, web/index.html]
---

## Context
ADR-051 made the surfaces of the LCARS theme neutral and kept the LCARS shapes on the panel only. A second external review found that the window title bars had lost the LCARS identity: they looked like a generic dark desktop. The window content and the settings app were correct.

## Decision
This decision replaces ADR-051. In the LCARS theme of the shell, the surfaces are neutral and each color has one role. The surfaces are the desktop (#100b1a), a window (#1b1624), and a raised part (#292136). The accent --focus, amber by default, marks the active item, the selection, and the main action of a form. Lavender marks navigation and identity: the open page of an app, the session button, and the shell icons. Coral marks a window that needs attention. --ok and --bad stay for a state, with a text. The LCARS shapes are on the chrome of the shell only: the elbow of the apps button, the cap of the session button, the workspaces as one segmented bar with rounded ends (the accent color fills the current workspace), and an endcap at the left of each window title bar, under the icon of the window. The endcap is in the accent color on the window that has the focus and in a muted lavender on other windows. A window has a 1px border, in the accent color when it has the focus, and a radius of 6px; a menu and a dialog have a radius of 6px. The content of a window has no LCARS shapes.

## Consequences
A new component uses the tokens and adds no bright fill. It shows the active item with a line or a marker in --focus, not with a fill in --focus; the current workspace and the endcap of the focused window are the exceptions. Only the main action of a form (button type submit) is a pill in the accent color. A border of a control uses --lcars-control, which has a contrast of 3:1; --lcars-border is for a divider only. A shell icon on the endcap is dark (filter icon-tint-dark of index.html). A rule in lcars.css that can match an element of the login screen excludes .login or .lcars-page: the login screen must stay pixel-identical.
