# Changelog

This file lists the changes in farglass in plain language, the newest first.
The version numbers are the release tags.
One `vX.Y.Z` tag builds the release tarballs and the `farglassctl` binary of that version.

---

## Unreleased

### Added
- The GTK and KDE apps of the host follow the dark or light theme of farglass, and change at once. At the end of the web session, your own theme of the local desktop comes back.

## v0.3.0 — 26 November 2026 — Microphone and camera for host apps

### Added
- The apps of the host can use the microphone of your computer, for example for a video conference. Click Mic off in the Media menu of the panel to turn it on. To turn the microphone off for all users, set `microphone = false` in `/etc/farglass/farglassd.conf`.
- The apps of the host can use the camera of your computer, for example for a video conference. Click Cam off in the Media menu of the panel to arm the camera. The browser opens the camera only while an app of the host uses it. The camera uses at most half of your uplink, and less on a slow network. Thus it does not delay your keys and clicks. In Firefox on the host, set `media.webrtc.camera.allow-pipewire` to true in `about:config`. To turn the camera off for all users, set `camera = false` in `/etc/farglass/farglassd.conf`.

### Changed
- The sound, the microphone, and the camera are in one Media menu in the panel, thus the panel uses less space. With the menu closed, the Media button shows Mic on, Cam on, Cam ready, or Muted.

### Fixed
- A GTK window with no header bar of its own no longer shows two title bars. Before, GTK drew a title bar inside the frame of farglass.

## v0.2.2 — 25 November 2026 — Network check app

### Added
- The Network check app shows at a glance if the host reaches the internet and DNS. It also shows if the hosts and ports that you need are available or blocked.
- The internet check uses HTTPS to `1.1.1.1` and `google.com`. If the session has `https_proxy`, the app also checks the internet through that proxy.
- You can add ICMP and TCP checks to the app, each with its own timeout. The app keeps them in `~/.config/farglass/netchecks.toml`.
- An administrator can give checks to all users in `/etc/farglass/netchecks.toml`. The app shows them read-only.
- All checks run when the app opens. Select Check again to run them again, or Check on a row to run one check.
- Settings > Network has the button Check connections, which opens the Network check app.
- If your home folder has the file `.egressip`, the Network check app shows its first line at the top as the own IP address.

## v0.2.1 — 25 November 2026 — Terminal tabs, split panes, settings, and bookmarks

### Added
- Settings > Terminal sets the color profile, the font, the font size, and the scrollback of the terminal. The color profiles are Solarized dark, Solarized light, Gruvbox dark, Dracula, Nord, and Tango light. The default follows the desktop theme.
- A change in Settings > Terminal applies at once to each open terminal in each tab of your session.
- A terminal window can have tabs, and each tab has its own shell. To open a tab, select + in the tab bar, or press Alt+Shift+Enter. Press Alt+PageDown or Alt+PageUp to go to the next or the previous tab.
- To close a tab, select × on the tab, middle-click the tab, or type `exit` in its shell. Closing the window stops the shells of all its tabs.
- The tabs and their scrollback stay after a reload and show the same in each browser tab.
- Settings > Terminal sets when the tab bar shows, where a new tab opens, and if a tab closes when its shell stops.
- A terminal tab can show several shells side by side in split panes. Press Alt+Shift+\ to split a pane to the right, or Alt+Shift+- to split it down. You can also right-click the terminal and select Split right or Split down.
- Press Alt+Shift with an arrow key to go to the next pane. Drag the border between two panes to change their size, and double-click it to make the two panes equal again.
- To close a pane, type `exit` in its shell, or right-click it and select Close pane. The panes and their sizes stay after a reload.
- Settings > Terminal can dim the panes that do not have the focus, and can give the focus to the pane under the pointer.
- A bookmark can open a terminal window. Select Terminal on the host as the browser of the bookmark, and type an ssh URL, for example `ssh://admin@192.0.2.1:2323`, or a folder, for example `~/src/project`.
- An ssh bookmark starts `ssh` to the host, with the user and the port of the URL. When `ssh` stops, press Enter in the terminal to connect again.
- A folder bookmark starts your shell in that folder. If the folder does not exist on the host, the Bookmarks app shows the problem on the bookmark.

### Changed
- The rows of the Bookmarks app no longer show Edit and Delete buttons. Select Edit at the top to show them, and select Done to hide them again.

## v0.2.0 — 24 November 2026 — Bookmarks, system tray, and settings with a filter and graphs

### Added
- The Bookmarks app keeps URLs with short names. Each bookmark opens in Firefox, Chromium, Chrome, or Brave on the host, in a profile of that browser, or in a private window. A bookmark can also open in a new tab of the browser that shows farglass.
- The bookmarks are in the file `~/.config/farglass/bookmarks.toml`. Copy the file to use the same bookmarks on a different host. A change by hand shows in the app, and a save from the app keeps the comments of the file.
- To find the bookmarks by their short names in the launcher, select Show in launcher search in the Bookmarks app. This option is off by default.
- The panel has a system tray. A program that stays in the tray when you close its window, for example Bitwarden or Seafile, shows an icon there. Click the icon to get the window back. Before, such a program had no icon, and you had to start it again to get its window.
- Right-click an icon in the system tray to open the menu of its program, for example to lock or to quit it. The menu has the check marks and the submenus of the program.
- Alt+Shift+Space opens and closes the launcher on Windows and Linux. The key works in each browser and needs no F1 key, which many laptops reach only with Fn.
- The settings have a filter above the list of pages. Type a word, for example `timezone` or `hostname`, to see the settings of all pages that match. Escape clears the filter, and Ctrl+F in the Settings window goes to it.
- Settings > Resources shows the use of the whole host: the CPU, the memory, and the network of all users and programs.
- Settings > Resources shows a graph of the last 5 minutes for each number of the host and of farglass. Move the pointer over a graph to read the value at a time. The graphs have the accent color of the theme.
- Settings > Resources tells when its numbers are old, for example after the connection to the host stopped.

### Changed
- Each page of the settings shows its settings in groups with a title.
- In a narrow Settings window, the filter and the list of pages are at the top, and the tables scroll in their own box. Before, the list of pages kept its width and left little space for the page.

### Fixed
- The text fields of a settings page now have the same width. Before, the fields for the hostname and the display name had different widths.
- The mark This session in the list of sessions stays in one piece. Before, it broke into two lines in a narrow window.
- A click in a Linux app that does not have the focus now makes the app active before it gets the click. Before, a click in the address bar of a second browser window showed no text cursor, and the window looked inactive until you typed.
- Clicks in Firefox on the host now reach Firefox. Before, Firefox on Wayland ignored the clicks in its window, for example on a tab or in the address bar.

## v0.1.9 — 24 November 2026 — Dock, keyboard layout indicator, and clock options

### Added
- The panel shows the keyboard layout of the Linux apps, for example US, DE, or AT. A click on it selects Automatic, a layout that you used before, or opens the full list. With Automatic in Chromium and Edge, a change of the layout on your computer shows in a few seconds.
- Settings > Appearance > Panel has a dock on the left, as an alternative to the bar at the top. The dock shows one icon for each app, with a dot for each window. A click starts the app, shows its last window, or goes to its next window. Right-click an icon to pin it to the dock. The pins are the same as the pins of the launcher.
- Settings > Date and time > Clock in the panel selects a 24-hour or a 12-hour clock. The default is the format of the language of your browser.
- The panel can also show the time as a crockford timestamp: the date and the time in UTC in five characters, for example `66X82`. To show it, select the check box in Settings > Date and time.

## v0.1.8 — 24 November 2026 — Keyboard layout, leave prompt, and full screen

### Added
- Settings > Keyboard has a layout for the Linux apps, for example German or Swiss French. The Linux apps then type the characters that are on your keys. In Chromium and Edge, farglass uses the layout of your keyboard by default. In other browsers, the default is English (US), and you can select a different layout.
- The browser asks before it closes or reloads the tab of the desktop, for example after Ctrl+W or Cmd+W.
- The session menu has Full screen. In Chromium and Edge, Keyboard Lock then gives Ctrl+W, Ctrl+T, Ctrl+N, Alt+Tab, and the Super key to farglass, not to the browser or the operating system. To leave full screen, select Leave full screen or hold Esc.

## v0.1.7 — 24 November 2026 — An ssh-agent for each web session

### Added
- Each web session has its own ssh-agent: load a key with `ssh-add`, and ssh and git in the terminals and apps of the desktop use it. The keys go when the session locks. To keep them during a lock, set `ssh_agent_clear_on_lock = false` in the configuration of farglassd. To turn the ssh-agent off, set `ssh_agent = false`.

## v0.1.6 — 23 November 2026 — Lock screen lockout fix and a longer idle lock

### Changed
- farglassd locks a session after 48 minutes with no keyboard or pointer activity, not after 15 minutes. If `idle_lock_minutes` is set in the configuration of farglassd, that value stays in effect.

### Fixed
- A lock screen or a login screen that stays open with no input, for example in a background tab, no longer blocks the login with "Too many failed logins". A login that ran out of time counted as a failed login also when no one typed a password.

## v0.1.5 — 23 November 2026 — Window border fixes and version code

### Changed
- The login, lock, and PIN screens show the version of farglass as a code, for example `FG-000104` for 0.1.4.

### Fixed
- The border of a Linux app window, for example Brave, now shows at the two bottom corners. Before, the app covered the round corners of the border.
- The border of a browser window on the host no longer disappears at the left or the right of a video, for example after a change of the focus or when another window moved over it. A video that the client plays at the edge of the window covered the border in some browsers.

## v0.1.4 — 22 November 2026 — farglassctl updates itself

### Added
- `farglassctl update` also installs the `farglassctl` of the new release. Before, farglassctl stayed at the version of the install. An older farglassctl does not do this, so the update to this release keeps the old farglassctl one more time.

### Fixed
- A Linux app window now takes the size of the app after a resize, also when the app selects its own size, for example a terminal that snaps to its text cells. Before, the frame kept the size of the drag, and the app drew a smaller window in it.
- An X11 window now stays in the smallest and the largest size of the app.
- The border of a Brave or Chromium window no longer disappears at its right edge, for example while the host draws a YouTube video in the smart mode. A part of the browser window covered the border.

## v0.1.3 — 22 November 2026 — X11 apps through Xwayland

### Added
- X11 apps, for example Java Swing apps, apps in Wine, and SecureCRT, can show as windows of the desktop through Xwayland.
- X11 apps are off by default. To turn them on, set `x11_apps = true` in the configuration of farglassd, and each user logs out and logs in again.
- The clipboard goes between X11 apps, the other apps, and your computer.
- X11 apps show their own pointer shapes, and their menus close after a click outside them.
- `farglassctl install` and `farglassctl update` install Xwayland.

### Fixed
- A Wayland app that draws its own pointer now shows that pointer. Before, the desktop showed the default arrow.
- farglassd uses a passkey library with a fixed URL parser (RUSTSEC-2026-0592). The old parser overflowed the stack on a nested `blob:` URL.

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
