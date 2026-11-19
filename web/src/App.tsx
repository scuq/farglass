import { createSignal, Match, onCleanup, Switch } from "solid-js";
import { ContextMenuHost } from "./menu/ContextMenuHost";
import { Desktop } from "./wm/Desktop";
import { LoginScreen } from "./LoginScreen";
import { type Connection, connect, endpointUrl } from "./proto";
import { reconnectDelay } from "./reconnect";
import { storeSession } from "./session";

/** The state of the connection to farglassd. */
type Stage =
  | { readonly kind: "connecting" }
  | { readonly kind: "login"; readonly connection: Connection; readonly notice: string | undefined }
  | { readonly kind: "desktop"; readonly connection: Connection; readonly user: string }
  | { readonly kind: "waiting"; readonly reason: string; readonly retryAt: number };

/** The functions of the shell that tests replace. */
export interface AppServices {
  connect?: () => Promise<Connection>;
  storeSession?: (ticket: string) => Promise<boolean>;
  logout?: () => Promise<void>;
}

/** Logs out through farglassd. Each tab of the user then gets the logout message. */
async function logout(): Promise<void> {
  await fetch("/session", { method: "DELETE", credentials: "same-origin" });
}

/**
 * The root of the shell. It connects to farglassd, shows the login screen if
 * the connection has no user, and then the desktop. After a break, it
 * connects again. The session cookie and the state store of the agent then
 * give the desktop back.
 */
export function App(props: AppServices) {
  const [stage, setStage] = createSignal<Stage>({ kind: "connecting" });
  const [now, setNow] = createSignal(Date.now());
  let attempt = 0;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let notice: string | undefined;
  let active: Connection | undefined;

  const open = () => {
    clearTimeout(retryTimer);
    (props.connect ?? (() => connect(endpointUrl())))().then(
      (connection) => {
        attempt = 0;
        active = connection;
        connection.onclose = (reason) => {
          if (connection !== active) {
            return;
          }
          if (connection.loggedOut) {
            // Connect again at once. The cookie no longer works, thus the login screen shows.
            notice = "You logged out.";
            open();
          } else {
            wait(reason);
          }
        };
        if (connection.user === null) {
          setStage({ kind: "login", connection, notice });
        } else {
          setStage({ kind: "desktop", connection, user: connection.user });
        }
        notice = undefined;
      },
      (error: unknown) => wait(error instanceof Error ? error.message : String(error)),
    );
  };

  const wait = (reason: string) => {
    const delay = reconnectDelay(attempt);
    attempt += 1;
    setStage({ kind: "waiting", reason, retryAt: Date.now() + delay });
    retryTimer = setTimeout(open, delay);
  };

  const connectNow = () => {
    if (stage().kind === "waiting") {
      open();
    }
  };

  // The countdown of the waiting screen.
  const clock = setInterval(() => setNow(Date.now()), 500);
  // A network that comes back, or a tab that becomes visible, connects at once.
  window.addEventListener("online", connectNow);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      connectNow();
    }
  });
  onCleanup(() => {
    clearInterval(clock);
    clearTimeout(retryTimer);
    window.removeEventListener("online", connectNow);
    active = undefined;
  });

  open();

  return (
    <main class="shell">
      <Switch>
        <Match when={stage().kind === "connecting"}>
          <p role="status">Connecting to the host</p>
        </Match>
        <Match when={stage().kind === "login" && stage()}>
          {(current) => {
            const { connection, notice } = current() as Extract<Stage, { kind: "login" }>;
            return (
              <>
                {notice ? <p class="notice" role="status">{notice}</p> : null}
                <LoginScreen
                  connection={connection}
                  onlogin={(user, ticket) => {
                    connection.user = user;
                    setStage({ kind: "desktop", connection, user });
                    if (ticket) {
                      // The cookie lets a reload, a new tab, and a new connection skip the login.
                      void (props.storeSession ?? storeSession)(ticket);
                    }
                  }}
                />
              </>
            );
          }}
        </Match>
        <Match when={stage().kind === "desktop" && stage()}>
          {(current) => {
            const { connection, user } = current() as Extract<Stage, { kind: "desktop" }>;
            return (
              <Desktop connection={connection} user={user} onlogout={() => void (props.logout ?? logout)()} />
            );
          }}
        </Match>
        <Match when={stage().kind === "waiting" && stage()}>
          {(current) => {
            const waiting = () => current() as Extract<Stage, { kind: "waiting" }>;
            const seconds = () => Math.max(0, Math.ceil((waiting().retryAt - now()) / 1000));
            return (
              <section class="closed" role="alert">
                <h1>No connection to the host</h1>
                <p>{waiting().reason}</p>
                <p role="status">Connecting again in {seconds()} s. Your session continues on the host.</p>
                <button type="button" onClick={connectNow}>
                  Connect now
                </button>
              </section>
            );
          }}
        </Match>
      </Switch>
      <ContextMenuHost />
    </main>
  );
}
