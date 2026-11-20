# farglass threat list

This list is the result of the security review of F035 (2026-11-20).
A second, short review after F051 to F059 added findings 13 to 17 (2026-11-20).
It covers the login, the session start, the channels of the shell, the web headers, the sockets, the passkey store, the video redirection, and the packages.
Each finding has a feature or a decision.
A path with a line number points to the code at the time of the review.

## Attackers

- Remote attacker with no password: reaches the HTTPS port of farglassd.
- Remote attacker with the password of a user.
- Web page in the client browser: a different site in a different tab.
- Web page in the host browser: a site that a user opens in Chromium or Brave in the session.
- Program of the user: an app or a script that runs as the user on the host.
- Different local user: an account on the host with no access to the session of the user.

## Entry points and controls

### HTTPS, the WebSocket, `/session`, and WebTransport

Threats: cross-site WebSocket hijacking, CSRF, theft of the cookie, framing, DNS rebinding.

Controls:

- The Origin header must be `https://` and the host of the request, or an origin of the configuration (`crates/farglassd/src/origin.rs`). The WebSocket, `/session`, and WebTransport check it.
- The session cookie is `__Host-farglass`: Secure, HttpOnly, and SameSite=Strict (`cookie.rs`).
- A login ticket has 32 random bytes, works one time, and ends after 60 s. farglassd keeps only its hash. A WebTransport ticket ends after 30 s.
- Each response has a CSP with `script-src 'self'` and `frame-ancestors 'none'`, COOP, COEP, CORP, `X-Frame-Options: DENY`, and no referrer (`http.rs`). HSTS comes only with a certificate of the administrator.
- In `dev_user` mode, the host of the request must be a loopback name.

### Login

Threats: guessing of passwords, a login with no second factor, denial of service of the login.

Controls:

- farglass-login runs PAM as root (ADR-003). It blocks an address (IPv6 by /64) or a user after five failures, and waits 2 s after a failure (`crates/farglass-login/src/limiter.rs`, `server.rs`).
- A blocked user gets no password prompt, and a correct password does not end a block. A login that stops after an answer, or passes the deadline of 60 s, counts as a failure (F051).
- farglass-login runs at most 32 logins at the same time, at most 4 from one address. 8 places are only for a browser with a device cookie (F051).
- farglassd closes a connection with no hello in 10 s, and a connection with no user after 10 minutes with no login. One address has at most 16 connections with no user (F051).
- A full table of the limiter keeps each active block, and refuses a new key (ADR-042).
- Each refusal goes to the security log (ADR-043).
- The failure text does not tell if the user exists. A login as root needs `--allow-root`.
- Only members of the group `farglass-users` log in (F058). A user outside the group gets the answer of a wrong password and no passkey step. A reconnect with a session cookie checks the membership again. A missing group or a read error refuses (ADR-042).
- After PAM, farglass-login waits until farglassd checks the passkey, before the session starts (ADR-040).
- An optional machine PIN comes before each login channel (F059). It is a filter, not a factor. Five wrong PINs block the address for 15 minutes. A PIN file that cannot be read refuses each PIN (ADR-042).
- The login socket has mode 0660 and the group farglass. farglass-login accepts only root, the user farglass, and its own user as the peer (F056).
- farglass-login keeps 7 capabilities (`CapabilityBoundingSet` in `farglass-login.service`), with `ProtectProc=invisible`, `UMask=0077`, and `PrivateIPC=yes` (F056).

### Passkeys

Threats: a login with the password only, theft of the passkey file, removal of the second factor.

Controls:

- webauthn-rs checks each passkey against the host name of the page (`crates/farglassd/src/passkeys.rs`).
- The file of each user has mode 0600 in a directory with mode 0700, and farglassd writes it atomically.
- A recovery code has 64 random bits and works one time. The file keeps only its SHA-256 hash.
- Only a missing file means "no passkey". A file that cannot be read keeps the second factor required, and no change overwrites it (fixed in F035, finding 1).
- A registration, a removal, or new recovery codes need a fresh passkey or a recovery code first, if the user has a passkey or a file that cannot be read (F053). The proof is valid for 5 minutes on one Settings window. Three wrong answers close the channel.

### Agent socket `/run/farglass/agent.sock`

Threats: a different local user gets the channels of a user. A program of the user replaces the agent and gets the keys, the clipboard, and the windows that the browser sends.

Controls:

- farglassd accepts an agent link only from root: the session worker connects, sends the claim of the uid, and gives the link to the agent through a private socket pair (F057). A link from the uid of the user is refused, unless farglassd runs with `--dev-user` or `--allow-user-agents` for development.
- The agent is not dumpable: a program of the user cannot attach a debugger to it or read its memory (F057).
- One uid has at most 4 links at the same time (F051).

### Channels of the shell

Threat: the shell, or a person at an unlocked tab, uses the host as the user.

Controls:

- A connection with no user, or with a locked session, can open only the login channel (`connection.rs`). The lock ends each connection of the user.
- A connection ends at the end of its session cookie, also through WebTransport, and when it misses a lock or logout event (F054).
- The agent refuses the login and the passkeys channels: farglassd serves them.
- The terminal runs the shell of the user, at most 64 terminals. This is full access as the user, by design.
- An OSC 52 clipboard write goes through only 3 s after a key or a click in that terminal. A later write asks the user (F055).
- The launcher starts only entries of the desktop files, each in a systemd scope.
- The state store accepts at most 8 MiB, with keys of at most 256 bytes. The clipboard has a size limit.
- The D-Bus channel permits a list of bus names. It is not a security boundary while the terminal exists (ADR-044).

### Video redirection

Threat: a web page in the host browser sends false player messages.
Controls:

- The native messaging manifest names the extension id. The socket of the agent is in the runtime directory of the user, mode 0700.
- page.js and relay.js talk through a MessagePort that page.js gives in a synchronous event before the page runs. relay.js takes only the first event (F052).
- The agent keeps at most 32 media sources of one browser connection, 8 SourceBuffers of each, an init segment of 4 MiB, and 64 MiB in all (F052).
- The client places a video only on a window of the browser process that sent it (the parent of the native messaging host), with the page title and only the browser name after it (F052).

### Packages

Controls:

- farglassd runs as an unprivileged user with an empty capability set, a system call filter, and `ProtectSystem=strict`.
- The session worker drops to the user with initgroups, setgid, and setuid, and an empty environment.
- The TLS key has mode 0600.
- farglass-login keeps 7 capabilities and runs with `ProtectProc=invisible`, `UMask=0077`, and `PrivateIPC=yes` (F056).

Gap: finding 17.

## Findings

| No. | Severity | Finding | Record |
|---|---|---|---|
| 1 | High | A read error of the passkey file other than "not found", for example with no free file descriptor, turned the second factor off. | Fixed in F035 |
| 2 | Medium | The block of a user applies only when farglassd gives the username. The login screen never does, thus an attacker with many addresses gets five guesses at each one. | Fixed in F051 |
| 3 | Medium | A connection waits for its hello with no time limit, and 32 logins that do not answer hold all login places. An attacker with no password can stop all logins and use up the file descriptors. | Fixed in F051 |
| 4 | Medium | Each script of a page in the host browser can send redirection messages. The agent keeps media, sources, and init segments with no limit, and the client places a video on each window whose title starts with the title that the page gives. | Fixed in F052 |
| 5 | Low | A person at an unlocked tab, or with a stolen cookie, can remove all passkeys with no new check. | Fixed in F053 |
| 6 | Low | A connection that misses a lock or logout event, after 64 events in its queue, keeps its channels. | Fixed in F054 |
| 7 | Low | A connection from a cookie continues after the cookie ends. | Fixed in F054 |
| 8 | Low | Output in a terminal (OSC 52) can write the clipboard of the client computer. | Fixed in F055 |
| 9 | Low | farglass-login runs as root with all capabilities and accepts each peer of its socket. | Fixed in F056 |
| 10 | Low | Each local user can open connections to the agent socket with no limit. | Fixed in F051 |
| 11 | Info | A program of the user can unlock the web lock screen. The replacement of the agent is fixed in F057. The passkey step shows that the password was correct. `dev_user` gives each local user a session. The CSP permits inline style elements. | ADR-044 |
| 12 | Info | `install-local.sh` puts the path of the repository in a shell string for `runuser`. | Fixed in F056 |
| 13 | Medium | farglassd counted each agent link from the session worker under the uid root, thus the cap of 4 links of F051 let at most 4 web sessions on the host. | Fixed after F057 |
| 14 | Low | farglassd has no cap on TCP connections and TLS handshakes for each address, and no rate limit on `POST /session` and `POST /transport`. Only the WebSocket connections with no user have a cap. | F060 |
| 15 | Low | The first passkey needs no proof. A person at an unlocked tab, or with a stolen cookie, can add a passkey to an account with no passkey, and the user is then locked out until the administrator removes the file. | F061 |
| 16 | Info | A failed login logs the typed username. A password typed in the username field goes into the journal. | F062 |
| 17 | Info | The root test starts farglass-login directly, not in its systemd unit: no test proves that PAM works with the capability set of F056. | F063 |
