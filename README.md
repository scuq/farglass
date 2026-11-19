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

- **Apps** opens the launcher. Type to search the apps, select one with the arrow keys, and press Enter.
- The workspace buttons and a button for each window of the workspace. A click on the window with the focus minimizes it.
- **Tile** places the windows of the workspace in columns.
- The clock, with the date in its tooltip. It shows the time of the client.
- Your username opens the session menu with **Log out**. **Lock** comes with the lock screen in a later version.

A tap of the Super key opens the launcher when the shell has Keyboard Lock.
Alt+F1 opens it on each client.

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

| Keys | Action |
|---|---|
| Alt+Tab, or Alt+` | Show the window switcher. Each Tab selects the next window. Release Alt to go to it. |
| Ctrl+Alt+Left, Ctrl+Alt+Right | Show the workspace on the left or on the right |
| Ctrl+Alt+Shift+Left, Ctrl+Alt+Shift+Right | Move the window to the workspace on the left or on the right |
| Super+Up, Super+Down | Maximize or restore, minimize |
| Super+Left, Super+Right | Snap to the left or the right half |
| Super+1 to Super+4 | Show a workspace |
| Super+T | Tile the windows of the workspace |
| Super (tap), or Alt+F1 | Open or close the launcher |

NOTE: The operating system of the client keeps Alt+Tab and the Super key, unless the shell is in fullscreen mode with Keyboard Lock.
Alt+` and the Ctrl+Alt keys reach the shell on each client.

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
The windows show in the shell with the tile codec (F025). Input to them comes with F029.

To look at the events of a real app:

```sh
cargo run -p farglass-surface --example serve -- /tmp/farglass-wayland-test /tmp
WAYLAND_DISPLAY=/tmp/farglass-wayland-test GDK_BACKEND=wayland gnome-calculator
```

The example prints each event and writes the last frame of each window to `/tmp/ID.ppm`.

## Settings

The settings app has these pages:

- Appearance: the theme, the accent color, and the text size. Each tab and each login of the user gets them.
- Date and time: the time of the host, the timezone, and the network time (timedate1).
- System: the hostname, the display name, and the facts of the host (hostname1).
- Network: the devices of NetworkManager with their state and addresses. The page does not change the network, because a change can stop the connection to farglass.
- Users and sessions: the sessions of logind. End stops a session after a confirm step.
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

## Login service

farglass-login does the PAM login and runs as root.
farglassd connects to its socket for each login attempt and relays each PAM prompt.
On a host, systemd starts it through `packaging/systemd/farglass-login.socket`.
The PAM service file is `packaging/pam.d/farglass`.

farglass-login refuses a login as root unless it starts with `--allow-root`.
After five failures from one address or for one user, it blocks that address or user for 30 seconds.
Each more failure doubles the block, up to 15 minutes.
After a failure, the answer comes 2 seconds later.

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
Then start farglassd with `--login-socket target/farglass/login.sock` and with no `--dev-user`.
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
