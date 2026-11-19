# farglass channel protocol

This document is the reference for the wire format of protocol version 1.
The crate `farglass-proto` defines the protocol.
The TypeScript files in `web/src/proto/generated/` come from that crate.

## Overview

One connection between the shell and the host carries numbered channels.
A channel carries one stream of work, for example one terminal.
The unit on the wire is a frame: a header of 8 bytes and a payload.

All frames of a channel go on that channel.
Frames of one channel stay in sequence.
Frames of different channels have no sequence between them.
Thus a transport can put each channel on its own stream, and a late frame on one channel does not block the others.

## Frame

All numbers are little-endian.

| Bytes | Field | Type | Meaning |
|---|---|---|---|
| 0 to 3 | channel | u32 | The number of the channel |
| 4 | type | u8 | The frame type |
| 5 to 7 | length | u24 | The length of the payload in bytes |
| 8 and above | payload | bytes | Raw bytes or JSON, as the frame type says |

The largest payload is 1048576 bytes (1 MiB).
A sender divides larger data into more than one data frame.
A receiver refuses a larger length when it reads the header, before it gets the payload.

On WebSocket, one binary message holds one full frame.
On a byte stream, for example a Unix socket, the frames follow one after the other.

## Frame types

| Number | Name | Payload | Meaning |
|---|---|---|---|
| 1 | open | JSON, `Open` | Opens the channel and selects its kind |
| 2 | ready | JSON, `Ready` | Accepts the channel |
| 3 | data | Raw bytes | Carries the data of the channel |
| 4 | control | JSON | Carries one message that the channel kind defines |
| 5 | close | JSON, `Close` | Closes the channel |

All other numbers are not valid in version 1.

## Channel numbers

- Channel 0 is the connection channel. It is always open and accepts only control frames.
- The shell opens channels with odd numbers.
- The host opens channels with even numbers.
- A connection uses each number one time only.
- The numbers that one side opens increase.

Each side can open a channel at all times, and the two sides cannot select the same number.

## Life of a channel

1. The opener sends an open frame on a new channel number.
2. The peer sends a ready frame to accept the channel, or a close frame to refuse it.
3. The two sides send data frames and control frames.
4. One side sends a close frame. The peer answers with a close frame if it did not send one.
5. After the two close frames, the channel does not exist.

The opener can send data frames and control frames before it gets the ready frame.
This saves one round trip.
If the peer refuses the channel, it drops those frames.

A side drops the frames that it gets on a channel after it sent a close frame, until the close frame of the peer arrives.

## Connection channel

Each side sends a hello message as its first frame:

```json
{"type":"hello","version":1}
```

If the versions are different, the receiver closes the connection.

When the user logs out, in this tab or in a different one, farglassd sends `{"type":"logged-out"}` on the connection channel and closes the connection.

If the connection already has a user, the hello message of farglassd has the field `user`:

```json
{"type":"hello","version":1,"user":"alice"}
```

With no `user`, the shell must log in on a login channel first.
Before the login, farglassd refuses each other channel with the reason `access-denied`.

## Login channel

The shell opens a channel of kind `login`.
farglassd serves this channel itself and relays the PAM conversation of farglass-login.
farglassd sends each event as a control message:

| Event | Meaning |
|---|---|
| `{"type":"prompt","prompt":1,"text":"Password: ","echo":false}` | A PAM module asks a question. If `echo` is `false`, the answer is secret. |
| `{"type":"info","text":"..."}` | A PAM module gives information. |
| `{"type":"error","text":"..."}` | A PAM module gives an error message. |
| `{"type":"confirm","sessions":[{"id":"2","kind":"wayland","seat":"seat0"}]}` | The password is correct, but the user has a local graphical session. The shell must ask the user. |
| `{"type":"success","user":"alice","uid":1000,"ticket":"..."}` | The login succeeded. The connection has the user from now on. The ticket gives the session cookie. |
| `{"type":"failure","message":"The login failed."}` | The login failed. |
| `{"type":"blocked","retry_after":30}` | Too many failures. A new attempt can start after `retry_after` seconds. |

The shell answers a prompt with a control message:

```json
{"type":"answer","prompt":1,"text":"secret"}
```

The shell answers a confirm event with `{"type":"confirm","accept":true}` or `{"type":"confirm","accept":false}`.
With `true`, the local graphical session ends, and the web session starts.
With `false`, the login stops with a failure, and the local session continues.

## State channel

The agent keeps a state store for the session: JSON values by key.
The shell keeps its state there, for example the open windows.
The store lives as long as the agent, thus a reload or a break of the connection does not lose it.

The shell sends these control messages:

| Request | Meaning |
|---|---|
| `{"type":"get","key":"windows"}` | Asks for the value. From now on, the channel also gets each change of the key. |
| `{"type":"set","key":"windows","value":[...]}` | Sets the value. `null` removes the key. |

The agent answers a get request, and tells each other state channel that asked for the key about a change:

```json
{"type":"value","key":"windows","value":[...]}
```

A key has 1 to 256 bytes.
All values together have a limit of 8 MiB.
A set request above the limit closes the channel with the reason `resource-limit`.

## Pty channel

A pty channel attaches to a terminal in the agent.
The terminal continues when the channel closes: jobs continue, and the output goes to the scrollback.

```json
{"kind":"pty","terminal":"6f1c...","cols":80,"rows":24}
```

If the agent has a terminal with the id `terminal`, the channel attaches to it.
If not, the agent starts the login shell of the user on a new pty, with `TERM=xterm-256color`.
With a systemd user manager, each terminal runs in its own scope `farglass-terminal-*.scope`.
A session has at most 64 terminals.

After the ready frame, the agent sends `{"type":"attached","new":true,"replay":0}`.
`replay` is the number of bytes of scrollback that follow as data frames, before the live output.
The scrollback has the last 1 MiB of output.

Data frames carry the input from the shell and the output from the agent.
The shell sends these control messages:

| Message | Meaning |
|---|---|
| `{"type":"resize","cols":120,"rows":40}` | Changes the size of the terminal. |
| `{"type":"ack","bytes":65536}` | The shell processed this many more bytes of output. |
| `{"type":"terminate"}` | Sends SIGHUP to each program of the terminal, and SIGKILL 3 seconds later. |

Flow control: the agent sends at most 256 KiB (`PTY_WINDOW`) of output that the shell did not acknowledge.
Then the agent stops to read the terminal until an acknowledgment comes.
Thus a program that writes much output waits for the browser, and the queues between the agent and the browser stay small.
With no attached channel, the agent reads with no limit and keeps the last 1 MiB.

When the program of the terminal stops, the agent sends `{"type":"exited","code":0}` or `{"type":"exited","signal":1}`, and closes the channel.
If no channel is attached at that time, the next attach gets the scrollback and the exit.

A close frame from the shell only detaches the channel.
To end a terminal, send `terminate`.

## Dbus channel

A dbus channel gives the shell a D-Bus connection of the agent, as the user:

```json
{"kind":"dbus","bus":"system"}
```

`bus` is `system` or `session`.
The agent permits only the bus names of its allowlist, as the destination of a call and as the sender of a subscription.
The default allowlist of the system bus has `org.freedesktop.DBus`, `hostname1`, `locale1`, `login1`, `NetworkManager`, `timedate1`, and `UPower`.
The default allowlist of the session bus has `org.freedesktop.DBus`.

The shell sends these control messages:

| Request | Meaning |
|---|---|
| `{"type":"call","id":1,"destination":"…","path":"…","interface":"…","method":"…","signature":"ss","args":[…],"interactive":false}` | Calls a method. With `interactive`, the service can ask polkit for an authentication. |
| `{"type":"subscribe","id":2,"sender":"…","path":"…","interface":"…","member":"…"}` | Subscribes to signals. `path`, `interface`, and `member` are optional. |
| `{"type":"unsubscribe","id":2}` | Ends a subscription. |

The agent answers each request with `{"type":"reply","id":1,"values":[…]}` or `{"type":"error","id":1,"name":"…","message":"…"}`.
`name` is the D-Bus error name, for example `org.freedesktop.DBus.Error.AccessDenied` for a name outside the allowlist.
Each signal of a subscription comes as `{"type":"signal","subscription":2,"path":"…","interface":"…","member":"…","values":[…]}`.
The subscriptions end with the channel.

A call runs in its own task in the agent, thus a slow service does not stop other channels.

The values use this JSON form:

| D-Bus type | JSON |
|---|---|
| Numbers, strings, object paths, signatures, booleans | As they are. A 64-bit number can come as a string. |
| Array, structure | Array |
| Dictionary | Object. A number key is its text form. |
| Variant | `{"type":"s","value":"…"}` |

A property is a call of `org.freedesktop.DBus.Properties`, for example `Get` with the signature `ss`.

## Polkit channel

The agent is the polkit authentication agent of its logind session.
At its start, it registers with polkit for the session in `XDG_SESSION_ID`.
logind elects the web session as the display session of the user, thus polkit also finds the agent for programs in the terminals.

The shell keeps a channel `{"kind":"polkit"}` open while the desktop is open.
When a program asks for an action that needs an authentication, the agent sends the request to each polkit channel of the user:

| Event | Meaning |
|---|---|
| `{"type":"begin","request":1,"action":"…","message":"…","identities":["alice","root"],"identity":"alice"}` | A new request. The first attempt uses `identity`. |
| `{"type":"prompt","request":1,"text":"Password: ","echo":false}` | The PAM conversation asks a question. |
| `{"type":"info","request":1,"text":"…"}` | The PAM conversation gives information or an error message. |
| `{"type":"failed","request":1}` | The attempt failed. The shell can start a new attempt. |
| `{"type":"done","request":1,"success":true}` | The request ended. |

The shell answers with these messages:

| Reply | Meaning |
|---|---|
| `{"type":"answer","request":1,"text":"…"}` | Answers the prompt. |
| `{"type":"start","request":1,"identity":"root"}` | Starts a new attempt as a different user, or after a failure. |
| `{"type":"cancel","request":1}` | Cancels the request. |

The agent runs `polkit-agent-helper-1` for each attempt, which tells polkit the result.
The agent cancels a request if no polkit channel is open, if the last one closes, after 5 minutes, or when polkit cancels it.
The agent accepts `BeginAuthentication` only from the owner of `org.freedesktop.PolicyKit1`, thus a different program cannot show a false prompt.

## Session cookie

A WebSocket message cannot set a cookie.
Thus the success event has a one-time ticket, which is valid for 60 seconds.
The shell sends the ticket to `POST /session` with the body `{"ticket":"..."}`.
The answer is `204 No Content` with this cookie:

```
Set-Cookie: __Host-farglass=TOKEN; Path=/; Secure; HttpOnly; SameSite=Strict
```

The browser sends the cookie with each later WebSocket handshake.
farglassd then gives the connection the user, and the hello message has the field `user`.
The session is valid for 12 hours.
`DELETE /session` logs the user out:

1. farglassd removes each cookie session of the user and the cookie.
2. Each browser connection of the user gets `logged-out` and closes.
3. farglass-login ends the web sessions of the user. Local sessions continue.

If a user has an agent but no browser connection for `idle_logout_minutes` (default 480), farglassd logs the user out in the same way.
farglassd keeps only the SHA-256 hash of each ticket and token, in memory.
After a restart of farglassd, each browser must log in again.

## Origin rule

farglassd accepts a WebSocket handshake, `POST /session`, and `DELETE /session` only if the `Origin` header is `https://HOST`.
`HOST` is the host of the request, with the port.
A request with no `Origin` header, or with a different origin, gets `403 Forbidden`.
The option `allowed_origins` adds more origins, for example the Vite server.
In `dev_user` mode, the host must also be `localhost` or a loopback address.
Thus a different site cannot use DNS rebinding to get a user with no login.

farglassd sends `success` after the agent of the user connected, at most 30 seconds after PAM accepted the user.

After `success`, `failure`, or `blocked`, farglassd closes the channel with the reason `done`.
If the shell closes the channel before the result, the attempt stops.
One login channel can be open at a time.
If the connection already has a user, farglassd refuses the channel with the reason `not-supported`.

## Agent link

farglass-session, the agent, connects to the Unix socket of farglassd, by default `/run/farglass/agent.sock`.
The link uses the same frames.
On a byte stream, the frames follow one after the other.

The kernel gives farglassd the uid of the agent (`SO_PEERCRED`).
farglassd gives an agent only the browser connections of its user.
Each user has one agent.
A new agent of a user replaces the old agent, for example after a restart.

One agent link carries the channels of all browser connections of its user.
Each shell numbers its channels from 1.
Thus farglassd gives each channel a new odd number on the agent link.
The agent sees only that number.

Channel 0 of the agent link carries other messages than channel 0 of the browser connection.
Each side sends a hello message first, as on the browser connection.
farglassd also sends these messages to the agent:

| Message | Meaning |
|---|---|
| `{"type":"attach","connection":7}` | Browser connection 7 of the user starts to use the agent. |
| `{"type":"detach","connection":7}` | Browser connection 7 stopped. farglassd sent a close frame for each of its open channels first. |

If a browser connection stops, the agent gets a close frame with the reason `cancelled` for each open channel.
If the agent stops, each browser connection gets a close frame with the reason `internal-error` for each open channel.
In version 1, only farglassd opens channels on the agent link.

## JSON messages

A receiver ignores a field that it does not know.
A receiver refuses a variant that it does not know, for example a new channel kind.

`Open` selects the channel kind with the field `kind`.
Version 1 has one kind:

| Kind | Function |
|---|---|
| `echo` | Sends each data frame back to the sender |
| `login` | A PAM login, served by farglassd. See the section on the login channel. |
| `state` | The state store of the session in the agent. See the section on the state channel. |
| `pty` | A terminal in the agent. See the section on the pty channel. |
| `dbus` | Method calls and signals on the system bus or the session bus, as the user. See the section on the dbus channel. |
| `polkit` | The polkit prompts of the session. See the section on the polkit channel. |

`Ready` is an empty object.

`Close` has the field `reason` and the optional field `message`:

```json
{"reason":"not-supported","message":"The channel kind is unknown."}
```

| Reason | Meaning |
|---|---|
| `done` | The work of the channel is complete |
| `cancelled` | The peer does not need the channel |
| `not-supported` | The receiver does not know the channel kind or an option |
| `not-found` | The object that the channel refers to does not exist |
| `access-denied` | The user does not have the permission |
| `resource-limit` | A limit of the receiver stops the channel |
| `protocol-error` | The peer sent a frame that the protocol does not permit |
| `internal-error` | The sender of the close frame has a defect or a failure |

## Errors

After a frame error, the position in the byte stream is unknown.
The receiver must close the connection.
The Rust side and the TypeScript side use the same error codes.

| Code | Cause |
|---|---|
| `unknown-frame-type` | The frame type is not a number from the table |
| `payload-too-large` | The length is more than 1048576 bytes |
| `invalid-connection-frame` | Channel 0 got a frame that is not a control frame |
| `truncated` | The message stops before the end of the frame |
| `trailing-bytes` | The message has more bytes after the frame |
| `invalid-json` | The JSON payload has a syntax error or does not match the message type |

## Change the protocol

1. Change the Rust types in `crates/farglass-proto/src/`.
2. Generate the TypeScript files again: `cargo run -p farglass-proto --example gen-ts`
3. Add a test vector to `crates/farglass-proto/tests/vectors.json`.
4. Run `cargo test --workspace` and `npm --prefix web test`.

The Rust tests and the TypeScript tests read the same test vectors.
`cargo test` fails if the generated TypeScript files are old.
If a change breaks an old peer, increase `PROTOCOL_VERSION`.
