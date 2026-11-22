# Changelog

This file lists the changes in farglass in plain language, the newest first.
The version numbers are the release tags.
One `vX.Y.Z` tag builds the release tarballs and the `farglassctl` binary of that version.

---

## Unreleased

### Added
- Apps with only an X11 backend, for example Java Swing apps, Wine, and SecureCRT, now show as windows of the desktop. `farglassctl install` and `farglassctl update` install Xwayland for them. The clipboard, the close of a menu after a click outside it, and the pointer shapes of X11 apps do not work yet.

## v0.1.2 — 22 November 2026 — Login on a host with SELinux

### Fixed
- A login on a host with SELinux in enforcing mode, for example RHEL, now starts the desktop. Before, farglass accepted the password, and the login failed after 15 seconds. `farglassctl install` and `farglassctl update` load a policy module that permits the start, and `farglassctl uninstall` removes it.
- If the web session cannot start, the login fails immediately and not after 15 seconds.

## v0.1.1 — 22 November 2026 — Newest two releases of each distribution

### Changed
- farglass now supports the newest release and the release before it of each distribution: Debian 13 and 12, Ubuntu 26.04 LTS and 24.04 LTS, RHEL, Rocky Linux, and AlmaLinux 10 and 9, and Fedora 45 and 44.
- `farglassctl install` and `farglassctl update` refuse Ubuntu 22.04 and Fedora 41 to 43.

## v0.1.0 — 22 November 2026 — First release

### Added
- farglass shows a Linux desktop in a web browser: the browser draws the shell, and programs on the host supply the data.
- You log in with your Linux username and password through the PAM stack of the host, and you get a real logind session.
- A passkey can be a second factor for the login.
- The administrator can set a machine PIN that the browser must give before the login screen shows.
- farglass locks the session after 15 minutes with no keyboard or pointer activity, and only a login unlocks it.
- The desktop has windows that you can move, snap, resize, maximize, and minimize, four workspaces, an overview, and a layout picker.
- The launcher finds apps and settings pages, shows your pinned and most used apps first, and calculates and converts units.
- The keyboard shortcuts follow the conventions of your computer, and the settings have a list of them and an interactive tour.
- The shell has a terminal, settings, notifications, context menus, and the polkit password dialog.
- A terminal continues on the host when you close the tab, and shows the same shell with its output when you connect again.
- Linux apps of the host show as windows of the desktop, with sound and the clipboard.
- A video in Chromium on the host plays in the browser of the client.
- The connection uses WebTransport over HTTP/3 if the browser and the network permit it, and a WebSocket if not.
- farglass accepts only TLS 1.3, and makes a self-signed certificate if you configure none.
- Scripts cannot read the session cookie, and farglass checks the Origin of each connection and sends a Content Security Policy.
- farglass limits the login tries of each address and writes a security log.
- The themes are LCARS (the default), light, and dark.
- `farglassctl install` installs farglass from a signed release, on amd64 and arm64.
- The supported distributions are Debian 12 and later, Ubuntu 22.04 and later, RHEL, Rocky Linux, and AlmaLinux 9 and later, and Fedora 41 and later.
- `farglassctl update` installs a newer release and keeps the configuration and the users, and `farglassctl uninstall` removes farglass.
- `farglassctl user` controls who can log in, `farglassctl session` lists and ends web sessions, and `farglassctl pin` sets and clears the machine PIN.
- `farglassctl status`, `farglassctl doctor`, and `farglassctl logs` show the state, the faults with a hint for each, and the log.
