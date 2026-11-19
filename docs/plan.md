# farglass - a Linux desktop shell that the browser draws

Status: design plan.
No code exists.
The pmm records in `.project/` hold the decisions, the phases, and the features.

## Description

farglass is a desktop for a Linux host.
You open a URL in a browser on a different computer and log in.
That browser is the client browser.
The client browser draws the shell: the panel, the window manager, the launcher, the terminal, and the settings.
The host runs no GNOME and no KDE.
The host gives a real login session and the services for each user.

rcd, the sibling project, sends the pixels of a GNOME session to a native client.
That design has these costs:

- It depends on Mutter internals. A mode change can stop GNOME Shell, and a locked session stops the screen cast.
- Software encode adds 21 ms for each frame at 1080p and 101 ms at 4K on the development VM.
- H.264 video makes text less sharp.
- Each client operating system needs a native client.

farglass does the opposite.
The client browser draws the interface locally.
The text is sharp, and no video encode adds a delay.

These decisions are final:

- The client browser runs on a remote computer. A local kiosk is not in scope.
- Web-native apps (apps written for the farglass shell) come first. The design keeps a place for Linux GUI apps.
- The first apps are a terminal, a web browser, and desktop settings.

## Speed

The shell, the terminal, and the settings are fast.
The client browser draws them.
Only small data goes through the network: key presses, terminal bytes, and D-Bus values.

Linux GUI apps and the web browser run on the host and use the surface pipeline.
The pipeline must be fast on a host with no GPU.
It uses the techniques of RDP and ICA and does not send a window as one video.
The table shows the expected cost.

| Content | Host cost | Result |
|---|---|---|
| Text and interface | Very low: lossless tiles and a tile cache | Exact text, near local speed |
| Scroll | Very low: move commands | Smooth |
| Video in a web player | Near zero, if video redirection works | Native |
| Other video, for example DRM, canvas, and games | High: software encode of that region | Usable at a lower resolution |

Video is the last slice.
Until then, video in a window is slow.

## Prior art

No project found does all of this: a shell that the browser draws, and a real login session on the host.
The parts exist in different projects.

| Project | Use | Do not use |
|---|---|---|
| Cockpit | The session and security model: an unprivileged web server and a small root login helper. One bridge process for each login, which runs as the user. Multiplexed channels | The Python bridge, the administration interface |
| Puter, daedalOS | The window manager behavior, the launcher and app model | Their own user accounts, their virtual filesystem, the iframe browser |
| OS.js (no activity since 2022) | The separation of authentication, filesystem, and app packages | The code |
| Greenfield, wprs, Xpra | Window forwarding: each Linux app window is one stream, and the browser is the compositor | The Greenfield code (AGPL), the X11 assumptions |
| Selkies, linuxserver webtop | The encoder design: damage tracking, WebCodecs decode, a lossless pass for regions that do not change | Full-desktop streaming |
| RDP, Citrix ICA | Content classes with a codec for each, a tile cache, scroll detection, video for regions only, video redirection | The protocols and the native clients |
| rcd | The login code, the session worker, the link between the root daemon and the user agent, the lossless refinement, the pmm workflow | The Mutter dependency, the native client |

## Browser support

Chromium is the primary target.
Firefox and Safari must work, with fewer keyboard shortcuts.

| API | Chromium | Firefox | Safari |
|---|---|---|---|
| WebCodecs | Yes | Yes | Yes |
| WebTransport | Yes | Yes | Yes, since 26.4 (one source) |
| Keyboard Lock | Yes | No | No |
| Window Management | Yes | No | No |
| File System Access | Yes | No | No |
| Controlled Frame | ChromeOS only | No | No |

Keyboard Lock lets the shell get Super and Alt+Tab in fullscreen mode.
Without it, the operating system of the client keeps those keys.

## Architecture

```
Client browser (any computer)         Linux host
+--------------------------+          +------------------------------------------+
| farglass shell (PWA)       |  HTTPS   | farglassd        unprivileged, network     |
|  window manager, panel   |<-------->|   TLS, shell files, session cookies,     |
|  terminal                |  WebSocket   relay between browser and agent        |
|  settings                |  WebTransport     | Unix socket                     |
|  surface windows (M4)    |          | farglass-login   root, socket-activated    |
+--------------------------+          |   PAM login and session start only       |
                                      |        | systemd-run and PAM session      |
                                      | farglass-session  runs as the user         |
                                      |   in a logind session of type web        |
                                      |   terminals, D-Bus bridge, polkit agent, |
                                      |   notifications, files, launcher         |
                                      +------------------------------------------+
```

The host runs three programs.
`farglassd` is the web server.
`farglass-login` is the login helper.
`farglass-session` is the agent, one for each user with a session.

### Session and security model

The session behaves as a GNOME or KDE session does.

Login.
The login screen of the shell relays the Pluggable Authentication Modules (PAM) conversation.
All PAM methods work, for example a password and then a one-time code.
The code comes from `rcd/crates/rcdd/src/pam.rs`, and the rate limiter comes from `rcd/crates/rcdd/src/auth.rs`.
The PAM service file is `packaging/pam.d/farglass`.

Privilege separation.
`farglassd` listens on the network and runs as an unprivileged user with `DynamicUser`.
Its unit file starts from `rcd/packaging/rcdd.service`.
Only `farglass-login` runs as root.
`farglass-login` does the PAM login and starts the session, and nothing else.
This separation is stricter than in rcd, where the network daemon runs as root.

Real session.
`farglass-login` starts a transient unit that opens a PAM session with `pam_systemd`, class `user`, type `web`.
The unit changes to the user and starts `farglass-session`.
The code comes from `rcd/crates/rcdd/src/worker.rs` and `rcd/packaging/pam.d/rcd-session`.
`loginctl` shows the session.
The session gets `XDG_RUNTIME_DIR`, a user manager, a session D-Bus, cgroup limits, and audit records.

Agent link.
`farglass-session` connects to `/run/farglass/agent.sock`.
`farglassd` reads the peer uid with `SO_PEERCRED` and compares it with the user who logged in.
The pattern comes from `rcd/crates/rcdd/src/agents.rs`.

Persistence.
When you close the tab, the session continues.
The terminals continue to run.
When you connect again, the shell restores the windows.
An idle session logs out after a configured time.
The lock screen does the PAM login again.

Administrator actions.
There is no root bridge.
`farglass-session` registers as the polkit authentication agent of the session and relays the prompt to the shell.
Settings that change system services then authenticate as they do in GNOME.

Web protection.
The session cookie has `HttpOnly`, `Secure`, and `SameSite=Strict`.
`farglassd` checks the `Origin` header on each WebSocket and WebTransport connection.
The Content Security Policy permits no inline script.
Passkeys are a later second factor.

### Wire protocol

One connection carries numbered channels, as in Cockpit.
The channel types are `pty`, `dbus`, `fs`, `launch`, `notify`, and `surface`.
A frame is a small binary header (channel, type, length) and a payload.
Control messages are JSON.
Terminal data and video data are raw bytes.
`farglass-proto` defines the types one time and generates the TypeScript types from them.

The first transport is WebSocket.
WebSocket works in all browsers and is sufficient for the terminal and the settings.
Milestone M4 adds WebTransport.
With WebTransport, a late video frame does not block input, because each stream is independent.

### Shell

The shell uses TypeScript, SolidJS, and Vite.
SolidJS updates only the DOM nodes that change, so a window move does not cause a tree comparison.
The window manager is DOM.
It moves windows with CSS `transform` only and uses pointer capture.
It has snap, tile layout, workspaces, and Alt+Tab.
Menus use the Popover API and CSS anchor positioning.

The shell is a progressive web app (PWA) with a service worker.
The shell files load from the cache.
In fullscreen mode on Chromium, the shell uses Keyboard Lock.
Each app is an ES module that loads on first use.
The app interface is small: open a window, open a channel.

### Terminal app

The terminal uses xterm.js 6 with the WebGL renderer.
A thin interface around it lets ghostty-web replace it later.
The pseudoterminal is in `farglass-session`.
Each terminal runs in its own `systemd --user` scope.
The channel has flow control by byte count.
The agent keeps a scrollback buffer, and the shell gets it when you connect again.
Copy and paste use OSC 52 and the Async Clipboard API.

### Settings app

The settings app is a web interface on the `dbus` channel.
It has these pages:

- Appearance, from the farglass configuration
- Date and time, from `org.freedesktop.timedate1`
- Hostname and system information, from `org.freedesktop.hostname1`
- Network, from NetworkManager
- Users, sessions, and power, from `org.freedesktop.login1`

### Web browser app

The web browser app is Chromium on the host, shown through the surface pipeline (ADR-008).
The browser is in the Linux session and uses the host network.
It comes with milestone M4.
Before M4, the desktop has no browser app.
A link from the terminal or an app opens in a new tab of the client browser, with a label that it leaves the host.

The web platform does not permit all three of these at the same time: a page inside a farglass window, all sites, and native speed.
The design does not use these options:

- An iframe in a farglass window. Most sites refuse an iframe through `X-Frame-Options`.
- A proxy on the host that removes `X-Frame-Options`. Logins and many sites fail, and site isolation in the browser becomes weaker.
- A browser engine in JavaScript or WebAssembly. No usable engine exists, and a page cannot open raw network connections or use DRM.
- Controlled Frame. It is in ChromeOS only. If it becomes available on other platforms, examine it again.

### Surface pipeline

The surface pipeline shows Linux GUI apps in farglass windows (ADR-007).
`farglass-session` runs a headless Wayland compositor built on Smithay.
The compositor does not draw a desktop.
Each top-level window becomes one `surface` channel.
The shell stays the only window manager.

A video codec for each window costs too much CPU on a host with no GPU.
RDP and ICA are fast on such hosts because they do not send the screen as one video.
The pipeline uses their techniques, and the project tries each of them:

- Damage from the clients. Each Wayland app tells the compositor which rectangles changed. An idle window costs no CPU.
- Lossless tiles. Text and interface regions go as tiles with a fast lossless compressor, so text is exact.
- Lossy tiles with a later lossless pass. Images and gradients go as lossy tiles first and become exact when they stop.
- Mirrored tile cache. The host and the client run the same slot allocator, so the host knows the client cache with no acknowledgment. The host sends a slot number for a tile that the client has (ADR-011).
- Local move and resize. While the user drags a window, the shell sends nothing and shows a snapshot. One size message goes at the end.
- Adaptive frame rate. Apps get frame callbacks at the full rate after input and at a low rate after 2 s with no input.
- Scroll detection. The host sends a move command for the region and only the new strip.
- Local cursor. The client draws the cursor from a shape message, so the pointer has no network delay.
- Input on its own stream. With WebTransport, surface data does not block input.
- Frame acknowledgment. On a slow network, the host drops old state and sends the newest state only.
- Video codec for a region. A region that changes all the time uses H.264 or AV1 for that rectangle only, with WebCodecs decode on the client.
- Video redirection. An extension in the host Chromium sends the compressed stream of a web player to the client browser, which decodes it with its hardware.

The last two items are milestone M6.
Video redirection needs a spike first.
The page on the host must continue as if it plays while the client plays the video, and that is not proven.

## Files

```
Cargo.toml                 workspace, same settings as rcd
crates/farglass-proto/       channel protocol, TypeScript type generation
crates/farglassd/            HTTPS server, sessions, relay
crates/farglass-login/       root PAM helper, session start
crates/farglass-session/     agent: pty, dbus, polkit, notify, fs
crates/farglass-surface/     M4: Smithay compositor, encoder
web/                       shell: src/wm, src/panel, src/apps
packaging/                 systemd units, pam.d, configuration
spikes/                    end-to-end tests
.project/                  pmm records: features, decisions, phases
```

The conventions come from rcd.
The project uses the pmm workflow.
A commit subject is `ID: title`.
Documentation and comments follow the scraibe standard.
The test command is `cargo test --workspace`.

## Milestones

The pmm phases P01 to P07 hold these milestones, and the features F001 to F040 hold the work.

1. M0, skeleton. Create the workspace and the channel protocol. `farglassd` serves the shell through TLS. A `dev_user` mode works on loopback only. One echo channel works from the shell to the agent.
2. M1, login and session. Add the PAM conversation in the login screen, `farglass-login`, the logind session, the session cookie, connect again, logout, and the idle timeout.
3. M2, shell and terminal. Add the window manager, the panel, the launcher, the terminal app, and links that open in the client browser. This is the first usable desktop.
4. M3, settings. Add the `dbus` channel, the polkit agent, notifications, the lock screen, and the settings pages.
5. M4, surfaces. Add WebTransport, the Smithay compositor, tiles, the tile cache, scroll detection, surface input, the clipboard, Linux apps in the launcher, and the host browser.
6. M5, packages and protection. Make a `.deb` package for Debian 13. Add passkeys. Do a security review. Test Firefox and Safari.
7. M6, video and sound. Add the video codec for regions, sound from host apps, and video redirection after its spike.

## Verification

- Unit tests: `cargo test --workspace` and `vitest` test the protocol, the flow control, and the window manager logic.
- End-to-end test: a script in `spikes/` uses a temporary PAM directory, so it needs no root. Playwright logs in with Chromium, opens a terminal, runs `id`, and compares the uid.
- Session: `loginctl list-sessions` shows a session of type `web` for the user. `systemd-cgls` shows the terminals in the scope of the user. `ps` shows that only `farglass-login` runs as root.
- Connect again: close the tab while `sleep 600` runs, then open it again. The job and the scrollback are there.
- Protection: `farglassd` refuses a WebSocket connection with a different `Origin`. Incorrect passwords start the rate limiter. A polkit action shows a prompt in the shell.
- Speed: a key press shows in the terminal in less than 5 ms on loopback. A window move keeps the refresh rate of the screen. The shell loads from the cache in less than 300 ms.
- Surfaces: an idle window uses no host CPU. A scroll of a text page sends move commands and new strips only. Measure the host CPU time and the bytes for each frame.

## Caveats

- The browser keeps some shortcuts, for example Ctrl+W and Ctrl+T, when the shell is not in fullscreen mode or PWA mode. Install the shell as a PWA on Chromium to get the most shortcuts.
- M4 is the largest part. The project owns a compositor and a set of codecs.
- Before M6, video in a surface window is slow. After M6, video that redirection cannot take uses software encode on a host with no GPU.
- WebTransport with a self-signed certificate needs a certificate with a short life and a pinned hash. A certificate from a public authority does not have this limit.

## See also

- Cockpit: https://cockpit-project.org
- daedalOS: https://github.com/DustinBrett/daedalOS
- Greenfield: https://github.com/udevbe/greenfield
- Puter: https://github.com/HeyPuter/puter
- Selkies: https://github.com/selkies-project/selkies
- wprs: https://github.com/wayland-transpositor/wprs
- Xpra HTML5 client: https://github.com/Xpra-org/xpra-html5
