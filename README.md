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

## Login service

farglass-login does the PAM login and runs as root.
farglassd connects to its socket for each login attempt and relays each PAM prompt.
On a host, systemd starts it through `packaging/systemd/farglass-login.socket`.
The PAM service file is `packaging/pam.d/farglass`.

farglass-login refuses a login as root unless it starts with `--allow-root`.
After five failures from one address or for one user, it blocks that address or user for 30 seconds.
Each more failure doubles the block, up to 15 minutes.
After a failure, the answer comes 2 seconds later.

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

`npm --prefix spikes/e2e run echo` starts farglassd in development mode.
It opens the shell in two tabs and sends a message on the echo channel of each tab.
It passes if each tab gets only its own echo.

`npm --prefix spikes/e2e run login` starts farglass-login with a PAM stack that accepts the password `secret`.
It logs in through the login screen, first with a wrong password and then with `secret`.
Then it sends a message on the echo channel.
The tests need no root.

Open `https://localhost:8443` in the browser.
farglassd serves HTTP/2 and HTTP/1.1 through TLS.
The channel protocol uses the WebSocket endpoint `/ws`.

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
