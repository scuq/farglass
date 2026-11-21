---
id: ADR-049
title: LCARS is the default theme
status: accepted
date: 2026-11-21
tags: [web, ui]
superseded_by:
files: []
---

## Context
The farglass icon and the login screens (F070) use LCARS colors and shapes. The shell had only light, dark, and the system choice, in neutral colors.

## Decision
LCARS is the default theme of the shell, with the accent orange (the amber of the icon). Light, dark, and the system choice stay selectable. The theme is web/src/theme/lcars.css: it sets the shared color tokens and changes the shapes of the panel, windows, menus, and controls under :root[data-theme="lcars"]. index.html sets it before the first paint.

## Consequences
Each new component must look right in four themes; it uses the tokens (--bg, --fg, --muted, --line, --focus, --ok, --bad) and adds an LCARS rule only for a shape. The LCARS bar colors are decoration: a state keeps a text and --ok or --bad, and --bad stays distinct from coral. isDarkTheme is true for LCARS; code that draws its own colors asks isLcarsTheme.
