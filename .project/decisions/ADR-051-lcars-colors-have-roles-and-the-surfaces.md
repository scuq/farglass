---
id: ADR-051
title: LCARS colors have roles, and the surfaces are neutral
status: accepted
date: 2026-11-22
tags: [web, ui]
superseded_by:
files: [web/src/theme/lcars.css]
---

## Context
ADR-049 made LCARS the default theme. F071 gave almost each control a bright fill: amber, orange, copper, lavender, or coral. ADR-049 calls these colors decoration with no state. A review found the desktop too busy for a day of work. The login screen (F070) is correct and stays.

## Decision
In the LCARS theme of the shell, the surfaces are neutral and each color has one role. The surfaces are the desktop (#100b1a), a window (#1b1624), and a raised part (#292136). The accent --focus, amber by default, marks the active item, the selection, and the main action of a form. Lavender marks navigation and identity: the open page of an app, the session button, and the shell icons. Coral marks a window that needs attention. --ok and --bad stay for a state, with a text. The LCARS shapes stay on the panel only: the elbow of the apps button and the cap of the session button. A window, a menu, and a dialog have a radius of 6px. ADR-049 stays accepted; this decision replaces its statement that the bar colors are decoration.

## Consequences
A new component uses the tokens and adds no bright fill. It shows the active item with a line or a marker in --focus, not with a fill in --focus. Only the main action of a form (button type submit) is a pill in the accent color. A border of a control uses --lcars-control, which has a contrast of 3:1; --lcars-border is for a divider only. A rule in lcars.css that can match an element of the login screen excludes .login or .lcars-page: the login screen must stay pixel-identical.
