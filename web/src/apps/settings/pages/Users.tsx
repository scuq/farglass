import { createSignal, For, onCleanup, Show } from "solid-js";
import type { DbusClient } from "../../../dbus";
import { ConfirmDialog, createConfirm, Loading, type Outcome, OutcomeLine, runAction } from "../common";

const LOGIN1 = "org.freedesktop.login1";
const LOGIN1_PATH = "/org/freedesktop/login1";
const MANAGER = "org.freedesktop.login1.Manager";

export interface SessionRow {
  readonly id: string;
  readonly user: string;
  readonly uid: number;
  readonly kind: string;
  readonly state: string;
  readonly seat: string;
  readonly from: string;
  readonly service: string;
  readonly locked: boolean;
  readonly since: number;
}

export interface UserRow {
  readonly name: string;
  readonly uid: number;
  readonly state: string;
}

const KINDS: Record<string, string> = { web: "farglass", wayland: "Wayland", x11: "X11", tty: "Text", mir: "Mir", unspecified: "Other" };

/** Reports whether `session` is the web session of `user`, which this tab shows. */
export const isThisSession = (session: SessionRow, user: string | null) =>
  session.kind === "web" && session.service === "farglass-session" && session.user === user;

/** The users and the sessions of the host, from login1. Ending a session needs a confirm step. */
export function UsersPage(props: { dbus: DbusClient; user: string | null }) {
  const [sessions, setSessions] = createSignal<readonly SessionRow[] | null>(null);
  const [users, setUsers] = createSignal<readonly UserRow[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [outcome, setOutcome] = createSignal<Outcome>(null);
  const confirm = createConfirm<SessionRow>();

  const load = async () => {
    try {
      const [list] = (await props.dbus.call(LOGIN1, LOGIN1_PATH, MANAGER, "ListSessions")) as [[string, number, string, string, string][]];
      const rows = await Promise.all(
        list.map(async ([id, uid, user, seat, path]) => {
          const p = await props.dbus.getAllProperties(LOGIN1, path, "org.freedesktop.login1.Session").catch(() => ({}) as Record<string, unknown>);
          return {
            id,
            uid,
            user,
            seat: seat || String(p.TTY ?? ""),
            kind: String(p.Type ?? "unspecified"),
            klass: String(p.Class ?? "user"),
            state: String(p.State ?? "unknown"),
            from: p.Remote === true && typeof p.RemoteHost === "string" ? p.RemoteHost : "",
            service: String(p.Service ?? ""),
            locked: p.LockedHint === true,
            since: Number(p.Timestamp ?? 0) / 1000,
          };
        }),
      );
      // The sessions of the user managers are not sessions of a person.
      setSessions(rows.filter((row) => row.klass === "user" || row.klass === "greeter").sort((a, b) => a.since - b.since));
      const [people] = (await props.dbus.call(LOGIN1, LOGIN1_PATH, MANAGER, "ListUsers")) as [[number, string, string][]];
      const userRows = await Promise.all(
        people.map(async ([uid, name, path]) => {
          const state = await props.dbus.getProperty(LOGIN1, path, "org.freedesktop.login1.User", "State").catch(() => "unknown");
          return { uid, name, state: String(state) };
        }),
      );
      setUsers(userRows.sort((a, b) => a.name.localeCompare(b.name)));
    } catch {
      setError("logind is not available on this host.");
    }
  };
  void load();

  const stops: (() => void)[] = [];
  let closed = false;
  for (const member of ["SessionNew", "SessionRemoved"]) {
    props.dbus.subscribe({ sender: LOGIN1, path: LOGIN1_PATH, interface: MANAGER, member }, () => void load()).then(
      (stop) => (closed ? stop() : stops.push(stop)),
      () => {},
    );
  }
  onCleanup(() => {
    closed = true;
    for (const stop of stops) stop();
  });

  const terminate = (session: SessionRow) => {
    confirm.clear();
    void runAction(
      () => props.dbus.call(LOGIN1, LOGIN1_PATH, MANAGER, "TerminateSession", "s", [session.id], { interactive: true }),
      setOutcome,
      `Session ${session.id} of ${session.user} ended.`,
    ).then(load);
  };

  const since = (ms: number) => (ms > 0 ? new Date(ms).toLocaleString() : "unknown");

  return (
    <div class="settings-page">
      <h2>Users and sessions</h2>
      <Loading when={sessions()} error={error()}>
        {(list) => (
          <>
            <table class="settings-table">
              <caption>Sessions</caption>
              <thead>
                <tr>
                  <th scope="col">User</th>
                  <th scope="col">Kind</th>
                  <th scope="col">State</th>
                  <th scope="col">Seat or from</th>
                  <th scope="col">Since</th>
                  <th scope="col">
                    <span class="visually-hidden">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                <For each={list()}>
                  {(row) => (
                    <tr data-session={row.id}>
                      <td>
                        {row.user}
                        <Show when={isThisSession(row, props.user)}>
                          <span class="settings-badge">This session</span>
                        </Show>
                      </td>
                      <td>{KINDS[row.kind] ?? row.kind}</td>
                      <td>
                        {row.state}
                        {row.locked ? ", locked" : ""}
                      </td>
                      <td>{row.from || row.seat || "none"}</td>
                      <td>{since(row.since)}</td>
                      <td>
                        <button type="button" class="danger" onClick={() => confirm.ask(row)}>
                          End
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
            <table class="settings-table">
              <caption>Users with a session or lingering processes</caption>
              <thead>
                <tr>
                  <th scope="col">User</th>
                  <th scope="col">UID</th>
                  <th scope="col">State</th>
                </tr>
              </thead>
              <tbody>
                <For each={users()}>
                  {(row) => (
                    <tr>
                      <td>{row.name}</td>
                      <td>{row.uid}</td>
                      <td>{row.state}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </>
        )}
      </Loading>
      <OutcomeLine outcome={outcome()} />
      <Show when={confirm.pending()}>
        {(session) => (
          <ConfirmDialog
            title={`End session ${session().id} of ${session().user}?`}
            action="End the session"
            onconfirm={() => terminate(session())}
            oncancel={confirm.clear}
          >
            <Show
              when={isThisSession(session(), props.user)}
              fallback={<p>Each program in the session stops, and work that is not saved in it is lost.</p>}
            >
              <p>This is the session of this desktop. Each tab logs out, and each program of the session stops.</p>
            </Show>
          </ConfirmDialog>
        )}
      </Show>
    </div>
  );
}
