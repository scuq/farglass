# farglass

farglass shows a Linux desktop in a web browser. The client browser draws the
shell. Programs on the host supply the data.

## Parts

| Part | Path | Function |
|---|---|---|
| farglass-proto | `crates/farglass-proto/` | The channel protocol between the shell and the host. |
| farglassd | `crates/farglassd/` | The web server and the channel router. |
| farglass-login | `crates/farglass-login/` | The PAM login and the session worker. |
| farglass-session | `crates/farglass-session/` | The agent in the logind session of a user. |
| shell | `web/` | The shell in the browser: TypeScript, SolidJS, and Vite. |

`docs/protocol.md` is the reference for the channel protocol.
`docs/threats.md` lists the threats, the controls, and the findings of the security review.
The TypeScript protocol files in `web/src/proto/generated/` come from `farglass-proto`.
After a change to the protocol, generate them again:

```sh
cargo run -p farglass-proto --example gen-ts
```

## Requirements

- Rust stable. `rust-toolchain.toml` selects the version.
- Node.js 26 LTS. `web/.nvmrc` records the version.

## Build and test

Build and test the host programs:

```sh
cargo build --workspace
cargo test --workspace
```

Install, test, and build the shell:

```sh
cd web
npm ci
npm test
npm run build
```

Start the shell development server:

```sh
cd web
npm run dev
```

## Run farglassd

Build the shell, then start farglassd with the shell files and a directory that it can write to:

```sh
npm --prefix web run build
cargo run -p farglassd -- --web-root web/dist --state-dir target/farglass \
    --agent-socket target/farglass/agent.sock
```

In a second terminal, start the agent as your own user:

```sh
FARGLASS_AGENT_SOCKET=target/farglass/agent.sock cargo run -p farglass-session
```

The agent connects to farglassd, and farglassd writes the uid of the agent to the log.
If farglassd stops, the agent connects again when farglassd starts.

## Install on this host

`packaging/install-local.sh` installs farglass from this source tree, with the systemd units and PAM files:

```sh
sudo packaging/install-local.sh install --user USER
```

`--user USER` adds USER to the group `farglass-users`: only its members can log in.
The option can come more than one time.

The script builds first, as the owner of the source tree and with the PATH of its login shell: `cargo build --release` and `npm --prefix web run build`.
If the build fails, the script installs nothing.
`--no-build` installs the last build, and the script refuses a build that is older than its sources.
After an install, reload the tab of the shell. A change of the agent needs a new login of the user.
An agent from a version before F057 cannot connect to the new farglassd: end its web session with `loginctl terminate-session ID` (the script names it), and log in again.
`sudo packaging/install-local.sh uninstall` removes farglass.

## Development mode with no login

Until the login is ready, `--dev-user` gives each browser connection a user with no login:

```sh
cargo run -p farglassd -- --listen 127.0.0.1:8443 --web-root web/dist \
    --state-dir target/farglass --agent-socket target/farglass/agent.sock --dev-user "$USER"
```

CAUTION: Do not use `--dev-user` on a host that other persons can use.
Each connection from the host itself becomes that user.

farglassd accepts `--dev-user` only on a loopback address, and refuses connections from other addresses.
Start the agent as the same user, as in the previous section.
Then open `https://localhost:8443`.
The shell opens an echo channel to the agent.
Each message comes back with its round-trip time.

To change the shell with live reload, start farglassd as before and then start the Vite server:

```sh
npm --prefix web run dev
```

The Vite server sends `/ws` to `https://localhost:8443`.
To use a different farglassd, set `FARGLASSD_URL`.

## Browsers

Chromium, and browsers built on it such as Brave, Edge, and Chrome, are the primary target.
Firefox and Safari work, with these limits:

| Function | Chromium | Firefox | Safari |
|---|---|---|---|
| Connection | WebTransport, else WebSocket | WebTransport, else WebSocket | WebSocket |
| Video region of a Linux app (H.264) and sound (Opus) | WebCodecs | WebCodecs | WebCodecs |
| Passkeys | Yes | Yes | Yes |
| Keyboard Lock | Not used yet | Not available | Not available |
| Window Management (more screens) | Not used yet | Not available | Not available |

With no WebTransport, the shell uses the WebSocket at once, with no wait.
A browser that cannot decode H.264 tells the agent, and the agent sends tiles only.
A browser with no `AudioDecoder` plays no sound of the apps.

With no Keyboard Lock, the browser keeps its own shortcuts, for example Ctrl+T, Ctrl+W, Ctrl+N, Ctrl+Tab, and Cmd+Q on a Mac.
The shortcuts of the shell never use these keys: see Settings > Keyboard.
To give such a key to a Linux app, use the menu of the app.
With no Window Management, the desktop uses the window of the browser only.
Firefox and Safari can show a Paste button before the shell reads the clipboard for a paste into a Linux app.
The video redirection was tested only with Chromium as the client browser.
The passkey test runs in Chromium only, because only Chromium has a virtual authenticator for tests.

`npm --prefix spikes/e2e run browsers` runs the shell in Firefox and in WebKit, the engine of Safari, through Playwright.
In each browser, it logs in, uses the echo channel, a terminal, a window drag, a setting, gnome-calculator, and an animation as H.264 video, and logs out.
Install the browsers first with `npx --prefix spikes/e2e playwright install firefox webkit`.
`BROWSERS=webkit` runs one browser only.

## Browser security

After a login, the shell gets the session cookie `__Host-farglass`.
The cookie is HttpOnly, Secure, and SameSite=Strict, and scripts cannot read it.
A reload or a new tab uses the cookie and skips the login for 12 hours.
`docs/protocol.md` describes the cookie and the Origin rule.

farglassd accepts a WebSocket and a session request only from a page of the same origin.
Each response has these headers:

- `Content-Security-Policy` with only files of this server, no inline script, and no frames. The policy permits inline `<style>` elements, because the terminal makes them.
- `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`
- `Strict-Transport-Security`, only with a certificate from `cert` and `key`

With a self-signed certificate, farglassd sends no `Strict-Transport-Security` header.
That header would stop the browser from accepting the next self-signed certificate after 30 days.

For the Vite development server, add its origin to the configuration file:

```toml
allowed_origins = ["http://localhost:5173"]
```

## Desktop

The desktop has windows that you can move, snap, resize, maximize, and minimize, and four workspaces.
The state of the desktop is in the state store of the session.
A reload, a different tab, or a different browser of the same user shows the same windows.

The panel at the top has these parts, from left to right:

- **Apps** opens the launcher. Type to search the apps and the pages of the settings, select one with the arrow keys, and press Enter.
  A word finds a settings page by its name or a keyword, for example `dark`, `timezone`, or `reboot`.
  A calculation, for example `2 * (3 + 4)^2`, or a unit conversion, for example `10 km in mi`, `100 f to c`, or `2 GiB in MB`, is the first result. Enter copies its value.
  The calculator runs in your browser: `+ - * / % ^`, parentheses, `pi`, `e`, and `sqrt`, `abs`, `sin`, `cos`, `tan`, `ln`, `log`, `exp`, `round`, `floor`, and `ceil`.
- The workspace buttons and a button for each window of the workspace. A click on the window with the focus minimizes it.
  The button of a minimized terminal or Linux app has a dot: it pulses while the app is busy, with 5 % CPU or 20 kB/s of network, and is hollow while the app is idle.
  The pointer on the button of each terminal and Linux app, open or minimized, shows the CPU, the memory, and the network of the app.
  The agent reads them from the systemd scope of the app or the terminal, every 2 s, and only while a tab is open.
- **Overview** shows all workspaces with their windows. Each window has a letter: press it to go to that window, also on a different workspace. Escape closes the overview.
- **Tile** opens the layout picker. Each layout shows a preview with your windows: columns, grid, center, wide, and spotlight. The window with the focus gets the main place. Select a layout with the arrow keys or 1 to 5, and press Enter.
- The clock, with the date in its tooltip. It shows the time of the client.
- Your username opens the session menu with **Log out**. **Lock** comes with the lock screen in a later version.


A right click opens the menu of the shell:

| Place | Menu |
|---|---|
| The title bar of a window, or its button in the panel | Minimize, maximize, snap, move to a different workspace, close |
| The empty desktop | Open each app, tile the windows |
| A terminal | Copy, paste, select all, clear the scrollback |
| A text field | Cut, copy, paste, select all |

Shift+F10 and the ContextMenu key open the menu of the focused element.
Use the arrow keys and Enter to select an item, and Escape to close the menu.
As on a native desktop, the menu of the browser does not show.

To move a window, drag its title bar.
To snap a window, drag it to an edge of the desktop:

| Edge | Result |
|---|---|
| Top | Maximized |
| Left or right | The left or the right half |
| A corner | A quarter |

To change the size, drag an edge or a corner of the window.
Press Escape during a drag to stop it.

The shortcuts follow the conventions of your computer: the "Windows and Linux" scheme, or the "Mac" scheme on a Mac.
The Keyboard page of the settings changes the scheme, lists each shortcut, and starts an interactive tour.
Hold Alt+Shift, or Ctrl+Shift on a Mac, to see the shortcuts on the screen. The tooltips of the buttons and the menus show them too.

| Action | Windows and Linux | Mac |
|---|---|---|
| Open or close the launcher | Super (tap), or Alt+F1 | Ctrl+Shift+Space |
| Switch windows. Release Alt or Option to go to the selected one | Alt+Tab, or Alt+` | Option+Tab |
| Open the overview | Super+Tab, or Alt+Shift+G | Ctrl+Shift+G |
| Open the layout picker | Super+Z, or Alt+Shift+L | Ctrl+Shift+L |
| Maximize or restore, minimize | Super+Up, Super+Down, or Alt+Shift+M, Alt+Shift+H | Ctrl+Shift+M, Ctrl+Shift+H |
| Snap to the left or the right half | Super+Left, Super+Right, or Alt+Shift+[, Alt+Shift+] | Ctrl+Shift+[, Ctrl+Shift+] |
| Show the workspace on the left or the right | Ctrl+Alt+Left, Ctrl+Alt+Right | Ctrl+Shift+Left, Ctrl+Shift+Right |
| Move the window to the workspace on the left or the right | Ctrl+Alt+Shift+Left, Ctrl+Alt+Shift+Right | Ctrl+Shift+, and Ctrl+Shift+. |
| Show workspace 1 to 4 | Super+1 to Super+4, or Alt+Shift+1 to Alt+Shift+4 | Ctrl+Shift+1 to Ctrl+Shift+4 |
| Tile the windows in columns | Super+T | The layout picker |
| Open a new terminal | Alt+Shift+T | Ctrl+Shift+T |
| Open a new window of the app with the focus | Alt+Shift+N | Ctrl+Shift+N |
| Close the window with the focus | Alt+Shift+W | Ctrl+Shift+W |

NOTE: The operating system of the client keeps Alt+Tab and the Super key, and the browser keeps Ctrl+T, Ctrl+N, and Ctrl+W, or Cmd+T, Cmd+N, and Cmd+W.
In fullscreen with Keyboard Lock, they reach the shell, and Ctrl+T, Ctrl+N, and Ctrl+W, or Cmd+T, Cmd+N, and Cmd+W, act in the windows of the shell.
A terminal and a Linux app keep them.
No shortcut takes a key of the browser, an input method, a screen reader, AltGr, or Option characters on a Mac.

## Terminal

**Terminal** in the launcher opens a shell on the host, as your user.
Each terminal window has its own shell in its own systemd user scope.
The shell continues when you close the tab or when the connection breaks.
After a reload or the next connection, the window shows the same shell with its output.
Closing the window ends the shell and its jobs.
If the shell stops, press Enter in the window to start a new shell.

| Keys | Action |
|---|---|
| Ctrl+Shift+C | Copy the selected text |
| Ctrl+Shift+V, or Shift+Insert | Paste |

A program on the host can put text on the clipboard of your browser with OSC 52, for example vim or tmux.
The write goes through only in the 3 seconds after a key or a click in that terminal.
A later write, for example from a remote host over ssh, shows a question in the terminal: **Copy** writes the clipboard, **Ignore** drops the text.
A write in the scrollback that a reload shows again asks nothing.
A program cannot read your clipboard.

NOTE: The browser keeps some keys that terminal programs use, for example Ctrl+W, Ctrl+T, and Ctrl+N.
In fullscreen mode with Keyboard Lock (F017), these keys go to the terminal.

`npm --prefix spikes/e2e run term` tests the terminal in Chromium with optimized programs.
It measures the delay from each key press to its echo with the Performance API.
It fails at a 95th percentile of 5 ms or more.

## Administrator actions

farglass is the polkit authentication agent of the web session.
When a program asks for an administrator action, for example `pkexec` in a terminal or a change of the time zone, the shell shows a dialog.
The dialog shows the message of polkit and asks for the password of a permitted user.
Cancel or Escape stops the action.
With no open tab, polkit gets no answer, and the action fails.

## Linux apps

Each session runs a Wayland compositor with no screen.
A Wayland app that starts in a farglass terminal, for example `gnome-calculator`, connects to it through `WAYLAND_DISPLAY`.
The compositor gives each window, its title, and the changed parts of each frame to the shell.
Each window shows in the shell as a window of the window manager: a worker draws its tiles on a canvas.
The size, the focus, the close, the keyboard, and the pointer of the window go to the app, and the cursor of the app shows on it.
`npm --prefix spikes/e2e run surface` starts gnome-calculator, and the shell must show its window.
`npm --prefix spikes/e2e run apps` starts gnome-calculator and VS Code side by side.
It moves each window by the own title bar of the app, types in the two windows, and switches the focus between them: each key must go to the window where it was typed.

A move or a resize of a Linux app window happens in the client browser.
During a resize, the window shows the last frame of the app, and the right and bottom edges of that frame stretch into the new area.
The app gets one new size at the end of the drag.
If the app refuses that size, for example because it is smaller than the smallest size of the app, the frame takes the size of the app.
`npm --prefix spikes/e2e run resize` tests this with gnome-calculator.

The launcher lists the apps of the host with the apps of the shell.
The agent reads the desktop entries in `applications` of `XDG_DATA_HOME` and of each directory in `XDG_DATA_DIRS`, in the language of `LANG`.
It does not list an entry with `NoDisplay`, `Hidden`, or `Terminal`, an entry for a different desktop in `OnlyShowIn` or `NotShowIn`, or an entry whose `TryExec` program is missing.
The icon comes from the hicolor theme or from `pixmaps`, as SVG or PNG.
A click starts the app with `WAYLAND_DISPLAY` of the session, each app in its own systemd --user scope `app-farglass-ID-RANDOM.scope`.
Its windows show the icon of its desktop entry, also in the panel.
If the app does not start, the launcher shows why.
`npm --prefix spikes/e2e run launcher` starts gnome-calculator from a desktop entry in the launcher.

The clipboard goes between the apps and your computer, as text or as a PNG image.
A copy in an app goes to the clipboard of your browser.
Before a paste key goes to an app (Ctrl+V, Ctrl+Shift+V, or Shift+Insert), the shell reads the clipboard of your browser and gives it to the apps.
Thus the browser asks one time for the permission to read the clipboard.
The terminal uses the clipboard of the browser too.
On a Mac, Cmd acts as Ctrl in the apps: Cmd+C copies and Cmd+V pastes.
A content of more than 16 MiB stays on its side.
`npm --prefix spikes/e2e run clipboard` pastes text from Chromium in VS Code, copies it back, and copies a result of gnome-calculator to Chromium and to VS Code.

A region that changes in each frame, for example a video player or an animation, goes as H.264 video.
The encoder counts the commits that change each tile.
A block of at least 6 tiles that changed in 12 of the last 16 commits, within 1.6 s, becomes the video region.
The rest of the window still goes as tiles.
OpenH264 encodes the region in software, and the browser decodes it with WebCodecs in the worker of the window.
When the region does not change for 500 ms, the video ends, and the region goes as exact tiles.
A blinking cursor or typing never starts a video.
A hardware encoder with VAAPI is not available yet.
`npm --prefix spikes/e2e run video` starts `spikes/e2e/fixtures/animation.py`, and the browser must decode its moving area as video.

The compositor sets the frame rate of each app through its frame callbacks.
After input or a change of the window, the window gets the full rate of about 60 frames each second for 2 s.
After that, an app that draws all the time, for example a spinner or a clock, gets the idle rate: 10 frames each second by default.
`FARGLASS_IDLE_FPS` in the environment of the agent sets the idle rate, and 0 turns it off.
A window with a video region keeps the full rate.
At the default idle rate, a video that starts with no input still becomes a video region: the detector needs about 8 frames each second.
A window that no browser shows, for example after you close the tab, gets no frame callbacks, thus the app draws nothing.
A popup follows its window.
`npm --prefix spikes/e2e run pace` measures the frame rate and the host CPU of `spikes/e2e/fixtures/animation.py --small`, with the limit and with no limit.

The sound of the apps plays in your browser.
The agent adds the audio sink `farglass` to the PipeWire server of the user.
The apps of the web session get `PULSE_SINK=farglass` and `PIPEWIRE_NODE=farglass`, thus PulseAudio and PipeWire clients play to it.
The agent encodes the sound as Opus at 48 kHz in stereo, 20 ms in each packet, and sends no packets after 1 s of silence.
The browser decodes the packets with WebCodecs and plays them in an AudioWorklet with a jitter buffer: the sound starts after 60 ms, and the delay stays below 200 ms.
The browser starts the sound only after a press or a key in the page.
The Sound button in the panel turns the sound off and on.
`npm --prefix spikes/e2e run sound` plays a tone from `spikes/e2e/fixtures/animation.py --tone`, and the browser must play it.

A video of a web page in the host browser plays in your browser, with the decoder of your computer (video redirection).
The extension in `extension/redirect` copies what the page gives to Media Source Extensions and the state of the video.
Its native messaging host, `farglass-session`, gives the copy to the agent through `$XDG_RUNTIME_DIR/farglass-redirect.sock`.
Your browser plays the same stream in a video element under the window of the host browser, at the time of the host.
The host hides the video and puts a black box under the controls of the page, and the black pixels of that rectangle are transparent in your browser: the controls stay on top.
The agent then encodes nothing for the video.
The two scripts of the extension talk through a private MessagePort, thus a script of the page cannot send messages to the agent.
Your browser places a video only on a window of the browser process that sent it, and only if the window title is the page title and the browser name.
The agent keeps at most 32 media sources of one browser, 8 SourceBuffers of each, an initialization segment of 4 MiB, and 64 MiB of video for a later tab.
The launcher starts Chromium and Brave with the extension, and the agent writes the native messaging manifest in a web session.
A browser that you start in a terminal needs `--load-extension=/usr/share/farglass/redirect-extension`.
Google Chrome does not load an extension from the command line.
Encrypted video (DRM) and Media Source Extensions in a worker stay as tiles.
A semi-transparent element of the page over the video shows in your browser as well as the key can make it: a light graph as translucent white, a dark gray panel as translucent dark gray.
A panel of nearly black, for example the settings menu of YouTube, is not distinct from the black under it, thus the video shows under its text with no panel.
Settings > Appearance > Videos in the host browser selects "Play on the host" for the exact page: the host then draws the video, with more load on the host and the connection.
`npm --prefix spikes/e2e run redirect` tests it with a local page, and `npm --prefix spikes/e2e run youtube` tries a YouTube video (network needed).

To look at the events of a real app:

```sh
cargo run -p farglass-surface --example serve -- /tmp/farglass-wayland-test /tmp
WAYLAND_DISPLAY=/tmp/farglass-wayland-test GDK_BACKEND=wayland gnome-calculator
```

The example prints each event and writes the last frame of each window to `/tmp/ID.ppm`.

In a web session, the agent gives `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE=wayland`, and `XDG_CURRENT_DESKTOP=farglass` to the systemd user manager and to D-Bus activation, as a local desktop does.
Thus a program that D-Bus starts, for example the desktop portal, finds the compositor.
The agent changes nothing if the user manager has a display already, for example from a local GNOME session.
`packaging/xdg-desktop-portal/farglass-portals.conf` selects the GTK portal backend for the farglass desktop.

## Settings

The settings app has these pages:

- Appearance: the theme, the accent color, the text size, and the motion. Each tab and each login of the user gets them.
  The motion follows the reduced motion setting of your computer, or Full, or Reduced. Reduced has no movement, no pulse, and no blinking cursor in the terminal. A change applies at once, with no reload.
  Videos in the host browser play in this browser (video redirection), or on the host.
- Keyboard: the shortcut scheme, the hints, the list of shortcuts, and the tour.
- Focus: the rules of focus protection, each on or off. See [Focus protection](#focus-protection).
- Date and time: the time of the host, the timezone, and the network time (timedate1).
- System: the hostname, the display name, and the facts of the host (hostname1).
- Network: the devices of NetworkManager with their state and addresses. The page does not change the network, because a change can stop the connection to farglass.
- Users and sessions: the sessions of logind. End stops a session after a confirm step.
- Resources: what the desktop costs. The CPU and the memory of farglassd and of the agent, the data rate of the connection of the tab, and a score: light, moderate, or heavy.
  A table lists each open terminal and Linux app with its CPU, memory, and network, the most CPU first. A click on a window name goes to that window.
- Power: Restart and Power off, each after a confirm step.

Each change of the host goes through polkit, thus the host can ask for an administrator password in the shell.
`npm --prefix spikes/e2e run settings` tests the app on the real system bus and changes only the appearance.

## Lock screen

Lock in the session menu locks the session in each tab of the user.
farglassd also locks it after 15 minutes with no keyboard or pointer activity in a tab of the user, and after `loginctl lock-session`.
While the session is locked, the browser gets no channel of the session, and the page has no desktop.
Your programs continue on the host.
The password of the user unlocks the session in each tab.
Log in as a different user removes the cookie of this browser only.
`idle_lock_minutes` in `farglassd.conf` changes the time, and 0 turns the idle lock off.

## Notifications

farglass is the notification service of the web session.
A notification from a program, for example `notify-send` in a terminal, shows at the top right of the shell.
A notification waits in the agent while no tab is open.
A click on the notification or on one of its buttons sends the action to the program.
The pointer on a notification stops its time. A critical notification stays until you close it.

`npm --prefix spikes/e2e run notify` tests the notifications in Chromium through gdbus.

## Focus protection

The desktop does not interrupt you while you work.
A notification waits while you type or move the pointer, and shows when you stop for 1.5 s, or after 30 s at the latest.
A critical notification shows at once.

A new window of a Linux app gets the focus only if you asked for it:

- You started the app in the launcher, in the last 30 s.
- Your last input was a click or Enter, in the last 10 s, with no typing after it. For example, Enter after a command in a terminal.
- The window is a dialog of the window with the focus.

Each other window opens behind the window with the focus, and its button in the panel has a mark until you look at it.
The Focus page of the settings turns each rule off or on.

## Write an app

Each app is a folder in `web/src/apps/`.
The name of the folder is the id of the app.
The shell finds each app from its `manifest.ts`, thus no list of apps needs a change.

`manifest.ts` gives the name, the icon, and the start function:

```ts
import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "clock",
  title: "Clock",
  description: "The time on the host",
  keywords: ["time"],
  icon,
  size: { w: 320, h: 200 },
  start: () => import("./Clock"),
});
```

The module of `start` has the component of the window as its default export.
Vite puts each app module in its own file.
The browser loads that file when the first window of the app opens, or when the pointer is on the app in the launcher.

The component gets an `AppContext` (`web/src/apps/types.ts`):

| Member | Use |
|---|---|
| `openChannel(open, handlers)` | Opens a channel to the agent. Closing the window closes the channel. |
| `state.watch(key, onvalue)`, `state.set(key, value)` | Read and write the state store of the session |
| `state.key(name)` | A key for the state of this window. A reload keeps it, and closing the window removes it. |
| `setTitle(title)` | Changes the title of the window |
| `openApp(id)` | Opens a window of a different app |
| `contextMenu(element, provider)` | Sets the context menu of an element. `provider` gives the items at the time of the click. |
| `onClose(handler)`, `onFocus(handler)` | Run code when the user closes the window, or when the window gets the focus |
| `windowId`, `user` | The id of the window and the user of the session |

After a reload or a break of the connection, the shell opens the windows again.
Each app then gets its state back through its keys.

## Connect again and log out

If the connection to the host breaks, the shell connects again after 1, 2, 4, 8, and then each 15 seconds.
It connects at once when the network comes back, or when you select **Connect now**.
The session continues on the host during the break.
The session cookie and the state store of the agent give the desktop back with no login.

**Log out** in the session menu ends the web session on the host and shows the login screen in each tab of the user.
If you have no browser connection for 8 hours, farglassd logs you out in the same way.
Programs in the session, for example terminals, continue until then.
To change the time, set `idle_logout_minutes` in the configuration file. The value 0 turns the idle logout off.

A session cookie is valid for 12 hours. Each connection ends when its cookie ends, and the next connection shows the login screen.
A connection that misses a lock or logout event, for example under high load, ends too. The shell then connects again and gets the current state.

## Login service

farglass-login does the PAM login and runs as root.
farglassd connects to its socket for each login attempt and relays each PAM prompt.
On a host, systemd starts it through `packaging/systemd/farglass-login.socket`.
The PAM service file is `packaging/pam.d/farglass`.

Only members of the group `farglass-users` can log in (`--allowed-group`).
To let a user log in:

```sh
sudo usermod -aG farglass-users USER
```

`install-local.sh install --user USER` creates the group and adds the user.
A user outside the group gets the same answer as for a wrong password, and the login never asks for the passkey.
Only the journal shows the cause: `event=login result=refused reason=not-in-group`.
If the group does not exist, farglass-login refuses each login and writes an error at the start.
farglassd also asks farglass-login at each connection with a session cookie: a user removed from the group gets the login screen.
An empty `--allowed-group ""` lets each user of the host log in, for development only.

farglass-login refuses a login as root unless it starts with `--allow-root`.
It accepts a connection to its socket only from root, the user `farglass`, and its own user.
Its unit keeps only the capabilities that PAM needs (`CapabilityBoundingSet` in `packaging/systemd/farglass-login.service`).
A PAM module that needs more, for example one that reads a hardware token, needs a drop-in file with the capability.
After five failures from one address or for one user, it blocks that address or user for 30 seconds.
Each more failure doubles the block, up to 15 minutes.
After a failure, the answer comes 2 seconds later.
IPv6 addresses count by /64 network.
A correct password does not end a block, and a blocked user gets no password prompt.

These limits keep one attacker from stopping the logins of other users:

- A login must be complete in 60 seconds (`--login-deadline`).
- One address runs at most 4 logins at the same time (`--max-per-address`).
- farglass-login runs at most 32 logins at the same time (`--max-attempts`).
  Of these, 8 (`--reserved-attempts`) are only for a browser that logged in before and has the device cookie `__Host-farglass-device`.
  This cookie gives no access.
- A login that stops after an answer, or that passes the deadline, counts as a failure.
- farglassd closes a connection that sends no hello message in 10 seconds.
  It also closes a connection with no user after 10 minutes with no login.
- One address has at most 16 connections with no user at the same time.
- One uid has at most 4 links to the agent socket at the same time.
- One address has at most 64 TCP or QUIC connections at the same time (F060).
- `POST /session` and `POST /transport` take 30 requests at once from one address, then 1 each second; more get 429 (F060).

The agent gets each link to farglassd from the session worker of its web session (F057).
The worker runs as root: it connects, names the uid of the session, and gives the open link to the agent through a private socket pair.
farglassd refuses an agent that connects as its user, unless it runs with `--dev-user` or `--allow-user-agents`.
Thus a program of the user cannot replace the agent and see the keys, the clipboard, and the windows that the browser sends.
The agent also turns off the debugger access of the user: other programs of the user cannot read its memory.
If farglassd restarts, the agent asks the worker for a new link.

After a login, farglass-login starts the web session of the user.
It runs `farglass-login --session-worker USER` as a transient systemd unit `farglass-session-UID`.
The worker opens a PAM session with the service `farglass-session` (`packaging/pam.d/farglass-session`).
pam_systemd makes a logind session of type `web`, and `loginctl` shows it.
The worker starts the agent as the user in that session.
A second login of the same user attaches to the open web session.
Different users have web sessions at the same time.

One user has one graphical session at a time.
If the user has a local graphical session (X11, Wayland, or Mir), the login screen asks first.
If the user confirms, farglass-login ends the local session through logind, and work that is not saved in it is lost.
If the user selects Cancel, the login stops and the local session continues.
Text sessions, for example SSH, continue in all cases.

`--session none` starts no session. Use it for development with no root, with an agent that you start yourself.

### Machine PIN

A host can ask for a PIN of 4 to 6 digits before the login screen.
The PIN is the same for each user of the host.
It stops random input: without the PIN, a browser cannot send a username, a password, a passkey, or a recovery code.
The PIN is off by default.

To set the PIN, or to change it:

```sh
echo 482913 | sudo -u farglass /usr/libexec/farglass/farglassd --set-pin
```

To remove the PIN:

```sh
sudo -u farglass /usr/libexec/farglass/farglassd --clear-pin
```

farglassd keeps a salted hash of the PIN in `/var/lib/farglass/pin`, mode 0600, and reads it at each check.
A new PIN applies at once, with no restart.
After a wrong PIN, the answer comes 1 second later.
Five wrong PINs block the address (IPv6 by /64) for 15 minutes.
A browser with a valid session cookie gets no PIN screen.
If the file cannot be read, farglassd asks for the PIN and refuses each one (ADR-042).
Each result goes to the security log once: `event=pin result=accepted` or `event=pin result=refused reason=wrong-pin`.

NOTE: The PIN is a filter, not a second factor.
4 to 6 digits give no protection without the block, and the password and the passkey stay necessary.

### Security log

farglassd and farglass-login write each security event to the journal with the target `farglass::security` (ADR-043).
Each line has the same fields:

```text
event=login result=refused reason=address-cap addr=203.0.113.7 user=- uid=-
```

A success has the level info, a refusal warn, and a fault of the host error with `reason=fault` (ADR-042).
A fault never lets a login pass: for example, an unreadable passkey file or user database refuses the login.
No password, PIN, recovery code, cookie, or ticket goes into the log.

To see the events:

```sh
journalctl -u farglassd -u farglass-login | grep farglass::security
journalctl -u farglassd -u farglass-login | grep 'result=refused'
```

A fail2ban filter, for example in `/etc/fail2ban/filter.d/farglass.conf`:

```ini
[Definition]
failregex = farglass::security: event=\S+ result=refused reason=(?!fault)\S+ addr=<ADDR>
journalmatch = _SYSTEMD_UNIT=farglass-login.service + _SYSTEMD_UNIT=farglassd.service
```

### Passkeys

A user can add a passkey as a second factor in Settings > Passkeys.
After that, each login asks for the passkey after the password: a phone, a security key, or the computer.
farglass-login waits after PAM until farglassd checks the passkey, thus no session starts before it.
A wrong passkey counts as a failed login for the block of the address and of the user.

The first passkey gives 8 recovery codes, and the page shows them one time.
On the login screen, "I lost my passkey: use a recovery code" takes a code in place of the passkey.
Each code works one time.
Settings > Passkeys makes new codes, and the old codes then stop working.
The removal of the last passkey also removes the codes, and the login then asks only for the password.

A user with a passkey must confirm each change first: a new passkey, a removal, or new recovery codes.
The Settings page asks for the passkey or a recovery code, and the confirmation is valid for 5 minutes in that Settings window.
Three wrong answers close the page. Thus a person at an unlocked tab, or with a stolen cookie, cannot remove the second factor.

farglassd keeps the passkeys of each user in `STATE_DIR/passkeys/UID.json` with mode 0600, by default in `/var/lib/farglass/passkeys`.
The file keeps only the SHA-256 hash of each recovery code.
To remove all passkeys of a user, for example after the loss of the passkey and of the codes, remove the file of the user as root.

A passkey belongs to the host name of the page where you added it.
It does not work on a different host name of the same host, and an IP address cannot have a passkey.
The login screen then names the host name of the passkeys, and a recovery code works on each host name.
Chromium, Firefox, and Safari use a passkey only on HTTPS with a valid certificate, or on `localhost`.
With the self-signed certificate of farglassd, use a certificate of the administrator first.
`npm --prefix spikes/e2e run passkey` adds a passkey with a virtual authenticator of Chromium, confirms a change with it, logs in with it and with a recovery code, and checks that a used code fails.

The tests of farglass-login need no root.
They use a PAM directory with `pam_exec` that accepts the password `secret`:

```sh
cargo test -p farglass-login
```

To start farglass-login with no root and no systemd, give it a socket and a PAM directory:

```sh
cargo run -p farglass-login -- --socket target/farglass/login.sock --pam-dir DIR
```

`DIR` must have a file `farglass` with a PAM stack that works for an unprivileged user.
Then start farglassd with `--login-socket target/farglass/login.sock`, `--allow-user-agents`, and with no `--dev-user`.
`--allow-user-agents` lets an agent that you start yourself connect as your user: only for development.
The shell shows the login screen with each prompt of the PAM modules.

## End-to-end tests

The end-to-end tests build all parts, start them, and use Chromium through Playwright:

```sh
npm --prefix spikes/e2e ci
npx --prefix spikes/e2e playwright install chromium
npm --prefix spikes/e2e test
```

`npm --prefix spikes/e2e run wm` tests the window manager.
It drags a window and measures the frames during the drag.
It fails if the 95th percentile of the frame interval is 25 ms or more.
It also tests a snap, a size change, Alt+Tab, the channel of an app after the moves, the launcher, the session menu, and a reload.

`npm --prefix spikes/e2e run echo` starts farglassd in development mode.
It opens the shell in two tabs and sends a message on the echo channel of each tab.
It passes if each tab gets only its own echo.

`npm --prefix spikes/e2e run transport` tests WebTransport, a terminal on it, and the fallback to the WebSocket.
It shows the echo round trip during terminal output on each transport.

`npm --prefix spikes/e2e run login` starts farglass-login with a PAM stack that accepts the password `secret`.
It logs in through the login screen, first with a wrong password and then with `secret`.
Then it sends a message on the echo channel.
The tests need no root.

### Test of the real session

The test of the real logind session needs root.
Build as your own user, then run the script with sudo:

```sh
cargo build -p farglassd -p farglass-login -p farglass-session
npm --prefix web run build
sudo spikes/session/check.sh
```

The script creates the temporary user `farglass-test` and removes it at the end.
It logs in as that user through Chromium. Then it examines the logind session of type `web` and the agent.
Then it selects **Log out** and examines that the web session and the agent stopped.
If your user has a local graphical session, the script also logs in as your user and selects Cancel at the confirm step.
It never ends your local session.

Open `https://localhost:8443` in the browser.
farglassd serves HTTP/2 and HTTP/1.1 through TLS on the TCP port, and HTTP/3 on the UDP port with the same number.
The channel protocol uses WebTransport on HTTP/3 if the browser has it and the UDP port is open, and else the WebSocket endpoint `/ws`.
The About app shows the transport of the tab.
To use HTTP/3 through a firewall, open the UDP port too, for example `8443/udp`.
`http3 = false` in the configuration file turns HTTP/3 off.

`farglassd --help` shows all options.
Each option also has an environment variable, for example `FARGLASSD_LISTEN`.
`packaging/farglassd.conf` is the configuration file with all options and their defaults.
farglassd reads `/etc/farglass/farglassd.conf` if the file exists, or the file that `--config` names.
`RUST_LOG=farglassd=debug` gives more log messages.

## TLS certificate

farglassd speaks only TLS 1.3, on TCP and on QUIC (F064).
It offers AES-128-GCM first, the fastest suite with AES instructions, then ChaCha20-Poly1305 for clients with no AES instructions.
It does not offer AES-256: speed comes first, and AES-128 is secure.
The key exchange is X25519, or secp256r1 for a client without X25519.
A browser older than TLS 1.3 support (Chrome 70, Firefox 63, Safari 12.1) cannot connect.

With no certificate in the configuration, farglassd makes a self-signed certificate.
It keeps the certificate and the key in the directory `tls` of the state directory.
The certificate is valid for 30 days.
farglassd makes a new one when the old one expires in less than one day.
The certificate has the hostname, `localhost`, `127.0.0.1`, and `::1`.
To add names, set `names` in the `[tls]` section of the configuration file.
At each start, farglassd writes the SHA-256 fingerprint of the certificate to the log.
Compare it with the fingerprint that the browser shows before you accept the certificate.

With the self-signed certificate, HTTP/3 uses a second self-signed certificate that is valid for 10 days, because a browser accepts a pinned certificate for at most 14 days.
farglassd keeps it in memory and makes a new one 2 days before its end.
The shell gets its SHA-256 hash from `POST /transport` on the TCP connection that you accepted, and gives it to WebTransport.
Thus no second warning shows.
The browser uses HTTP/3 for the pages only with your own certificate: then farglassd sends the header `Alt-Svc`.

To use your own certificate, give farglassd the two PEM files:

```sh
farglassd --tls-cert /etc/farglass/tls/fullchain.pem --tls-key /etc/farglass/tls/privkey.pem
```

You can also set `cert` and `key` in the `[tls]` section of the configuration file.
The certificate file holds the certificate chain, the server certificate first.
The key file holds the private key in PKCS #8, PKCS #1, or SEC 1 format.
The user of farglassd must be able to read the two files.

After a certificate client replaces the files, farglassd loads them again in less than one hour.
To load them immediately, send SIGHUP:

```sh
pkill -HUP farglassd
```

If the new files are not valid, farglassd keeps the old certificate and writes an error to the log.
