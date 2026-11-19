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

## Requirements

- Rust stable. `rust-toolchain.toml` selects the version.
- Node.js 26 LTS. `web/.nvmrc` records the version. Node.js 24.12 or later also works.

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
