import { createSignal, Match, Switch } from "solid-js";
import { Echo } from "./Echo";
import { LoginScreen } from "./LoginScreen";
import { type Connection, connect, endpointUrl } from "./proto";
import { storeSession } from "./session";

/** The state of the connection to farglassd. */
type Stage =
  | { readonly kind: "connecting" }
  | { readonly kind: "login"; readonly connection: Connection }
  | { readonly kind: "desktop"; readonly connection: Connection; readonly user: string }
  | { readonly kind: "closed"; readonly reason: string };

/**
 * The root of the shell. It connects to farglassd, shows the login screen if
 * the connection has no user, and then the desktop. In M1, the desktop has only the echo panel.
 */
export function App(props: { connect?: () => Promise<Connection>; storeSession?: (ticket: string) => Promise<boolean> }) {
  const [stage, setStage] = createSignal<Stage>({ kind: "connecting" });

  const open = (props.connect ?? (() => connect(endpointUrl())))();
  open.then(
    (connection) => {
      connection.onclose = (reason) => setStage({ kind: "closed", reason });
      setStage(
        connection.user === null
          ? { kind: "login", connection }
          : { kind: "desktop", connection, user: connection.user },
      );
    },
    (error: unknown) => setStage({ kind: "closed", reason: error instanceof Error ? error.message : String(error) }),
  );

  return (
    <main class="shell">
      <Switch>
        <Match when={stage().kind === "connecting"}>
          <p role="status">Connecting to the host</p>
        </Match>
        <Match when={stage().kind === "login" && stage()}>
          {(current) => {
            const { connection } = current() as Extract<Stage, { kind: "login" }>;
            return (
              <LoginScreen
                connection={connection}
                onlogin={(user, ticket) => {
                  connection.user = user;
                  setStage({ kind: "desktop", connection, user });
                  if (ticket) {
                    // The cookie lets a reload or a new tab skip the login.
                    void (props.storeSession ?? storeSession)(ticket);
                  }
                }}
              />
            );
          }}
        </Match>
        <Match when={stage().kind === "desktop" && stage()}>
          {(current) => {
            const { connection, user } = current() as Extract<Stage, { kind: "desktop" }>;
            return (
              <>
                <header class="shell-header">
                  <span>Logged in as {user}</span>
                </header>
                <Echo connection={connection} />
              </>
            );
          }}
        </Match>
        <Match when={stage().kind === "closed" && stage()}>
          {(current) => (
            <section class="closed" role="alert">
              <h1>The connection to the host closed</h1>
              <p>{(current() as Extract<Stage, { kind: "closed" }>).reason}</p>
              <button type="button" onClick={() => window.location.reload()}>
                Connect again
              </button>
            </section>
          )}
        </Match>
      </Switch>
    </main>
  );
}
