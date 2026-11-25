import { createSignal, For, onCleanup, Show } from "solid-js";
import type { NetcheckCheck, NetcheckGroup, NetcheckItem, NetcheckKind } from "../../proto";
import { ConfirmDialog, type Outcome, OutcomeLine } from "../settings/common";
import "../settings/settings.css";
import "./netcheck.css";
import type { AppContext } from "../types";
import { age, type CheckResult, type CheckStatus, NetcheckClient, STALE_MS, STATUS, statusOf, summarize } from "./netcheck";

/** A check in the form: the port and the timeout are text, "" for none. */
interface Draft {
  name: string;
  kind: NetcheckKind;
  host: string;
  port: string;
  timeout: string;
}

const EMPTY: Draft = { name: "", kind: "tcp", host: "", port: "", timeout: "" };

function draftOf(check: NetcheckCheck): Draft {
  return { name: check.name, kind: check.kind, host: check.host, port: check.port?.toString() ?? "", timeout: check.timeout?.toString() ?? "" };
}

function checkOf(draft: Draft): NetcheckCheck {
  const check: NetcheckCheck = { name: draft.name.trim(), kind: draft.kind, host: draft.host.trim() };
  if (draft.kind === "tcp" && draft.port.trim()) check.port = Number(draft.port);
  if (draft.timeout.trim()) check.timeout = Number(draft.timeout);
  return check;
}

/** The word and the mark of a state. The color only repeats them. */
function StatusMark(props: { status: CheckStatus }) {
  return (
    <span class="netcheck-status" data-level={STATUS[props.status].level} data-status={props.status}>
      <span aria-hidden="true" class="netcheck-mark">
        {STATUS[props.status].mark}
      </span>
      {STATUS[props.status].label}
    </span>
  );
}

/**
 * The Network check app (F111): checks of the internet, DNS, and the connections that
 * the user needs, from the host. All checks run when the window opens.
 *
 * The user adds ICMP and TCP checks in the file netchecks.toml in the agent. The checks
 * of the administrator come from /etc/farglass/netchecks.toml and are read-only.
 */
export default function NetcheckApp(props: { context: AppContext }) {
  const client = new NetcheckClient((open, handlers) => props.context.openChannel(open, handlers));
  const [outcome, setOutcome] = createSignal<Outcome>(null);
  /** The index of the check in the form: -1 for a new one, null with no form. */
  const [editing, setEditing] = createSignal<number | null>(null);
  const [draft, setDraft] = createSignal<Draft>(EMPTY);
  const [removing, setRemoving] = createSignal<number | null>(null);
  const [editMode, setEditMode] = createSignal(false);
  // The age of the results changes with the time.
  const [now, setNow] = createSignal(Date.now());
  const timer = setInterval(() => setNow(Date.now()), 10_000);
  onCleanup(() => clearInterval(timer));

  const state = () => client.state();
  const results = () => client.results();
  const items = (group: NetcheckGroup) => state()?.items.filter((item) => item.group === group) ?? [];
  const summary = () => summarize(state()?.items ?? [], results());
  const checking = () => Object.values(results()).some((result) => result.status === "checking");
  /** The time of the oldest result: the page shows the age of the run. */
  const oldest = () => {
    const times = Object.values(results())
      .filter((result) => result.status !== "checking")
      .map((result) => result.at);
    return times.length > 0 ? Math.min(...times) : null;
  };
  const stale = (result: CheckResult | undefined) => result !== undefined && result.status !== "checking" && now() - result.at > STALE_MS;
  const problemOf = (group: NetcheckGroup, index: number) => state()?.problems.find((problem) => problem.group === group && problem.index === index)?.message;
  /** The problems of a whole file: an index with no check. */
  const fileProblems = (group: NetcheckGroup) => state()?.problems.filter((problem) => problem.group === group && problem.index >= items(group).length) ?? [];
  const indexOf = (item: NetcheckItem) => Number(item.id.split(":")[1]);
  const change = (patch: Partial<Draft>) => setDraft((current) => ({ ...current, ...patch }));
  const editable = () => state() !== null && state()?.error === undefined && client.closed() === null;

  const save = (checks: readonly NetcheckCheck[], done: string) => {
    setOutcome(null);
    return client.save(checks).then(
      () => {
        setOutcome({ kind: "ok", text: done });
        return true;
      },
      (message: string) => {
        setOutcome({ kind: "error", text: message });
        return false;
      },
    );
  };

  const startEdit = (index: number) => {
    const check = state()?.user[index];
    setDraft(check ? draftOf(check) : { ...EMPTY });
    setEditing(index);
    setOutcome(null);
  };

  const submit = (event: SubmitEvent) => {
    event.preventDefault();
    const index = editing();
    if (index === null) return;
    const check = checkOf(draft());
    const list = [...(state()?.user ?? [])];
    if (index < 0) list.push(check);
    else list[index] = check;
    void save(list, `${check.name} is saved.`).then((saved) => {
      if (saved) setEditing(null);
    });
  };

  const remove = (index: number) => {
    const list = state()?.user ?? [];
    const name = list[index]?.name ?? "";
    setRemoving(null);
    void save(
      list.filter((_, each) => each !== index),
      `${name} is deleted.`,
    );
  };

  const Row = (rowProps: { item: NetcheckItem; editable?: boolean }) => {
    const result = () => results()[rowProps.item.id];
    const status = () => statusOf(results(), rowProps.item.id);
    const problem = () => (rowProps.item.group === "user" || rowProps.item.group === "admin" ? problemOf(rowProps.item.group, indexOf(rowProps.item)) : undefined);
    return (
      <li class="netcheck-row" data-id={rowProps.item.id} data-stale={stale(result()) ? "true" : undefined}>
        <StatusMark status={status()} />
        <div class="netcheck-what">
          <span class="netcheck-name">{rowProps.item.name}</span>
          <span class="netcheck-target">{rowProps.item.target}</span>
          <Show when={result()?.detail}>{(detail) => <span class="netcheck-detail">{detail()}</span>}</Show>
          <Show when={problem()}>{(text) => <span class="netcheck-problem">{text()}</span>}</Show>
          <Show when={stale(result())}>
            <span class="netcheck-age">Checked {age(result()?.at ?? 0, now())}: select Check again.</span>
          </Show>
        </div>
        <span class="netcheck-ms">{result()?.ms !== undefined ? `${result()?.ms} ms` : ""}</span>
        <button type="button" disabled={status() === "checking" || client.closed() !== null} onClick={() => client.run([rowProps.item.id])} aria-label={`Check ${rowProps.item.name} again`}>
          Check
        </button>
        <Show when={rowProps.editable && editMode()}>
          <button type="button" disabled={!editable()} onClick={() => startEdit(indexOf(rowProps.item))} aria-label={`Edit ${rowProps.item.name}`}>
            Edit
          </button>
          <button type="button" disabled={!editable()} onClick={() => setRemoving(indexOf(rowProps.item))} aria-label={`Delete ${rowProps.item.name}`}>
            Delete
          </button>
        </Show>
      </li>
    );
  };

  return (
    <section class="netcheck">
      <header class="netcheck-bar netcheck-top">
        <dl class="netcheck-summary" aria-label="Summary" aria-live="polite">
          <Show when={state()?.outgoingIp}>
            {(ip) => (
              <div>
                <dt>Own IP address</dt>
                <dd data-field="outgoing-ip">{ip()}</dd>
              </div>
            )}
          </Show>
          <div>
            <dt>Internet</dt>
            <dd>
              <StatusMark status={summary().internet} />
            </dd>
          </div>
          <div>
            <dt>DNS</dt>
            <dd>
              <StatusMark status={summary().dns} />
            </dd>
          </div>
          <Show when={summary().checks.total > 0}>
            <div>
              <dt>Your connections</dt>
              <dd data-field="checks-count">
                <span class="netcheck-status" data-level={summary().checks.done < summary().checks.total ? "unknown" : summary().checks.ok === summary().checks.total ? "ok" : "bad"}>
                  {summary().checks.ok} of {summary().checks.total} available
                </span>
              </dd>
            </div>
          </Show>
        </dl>
        <span class="netcheck-spacer" />
        <button type="button" disabled={checking() || state() === null || client.closed() !== null} onClick={() => client.reload()}>
          {checking() ? "Checking" : "Check again"}
        </button>
      </header>
      <p class="netcheck-note">
        The checks run on the host, not on this computer.
        <Show when={oldest()}>{(at) => <> Last check: {new Date(at()).toLocaleTimeString()} ({age(at(), now())}).</>}</Show>
      </p>
      <Show when={summary().dns === "failed" || summary().dns === "timeout"}>
        <p class="netcheck-alert" role="alert">
          Name resolution fails. Each check of a host name can fail for this cause: correct DNS first.
        </p>
      </Show>
      <Show
        when={state()}
        fallback={
          <Show when={client.closed()} fallback={<p role="status">Loading</p>}>
            {(message) => (
              <p class="netcheck-alert" role="alert">
                {message()}
              </p>
            )}
          </Show>
        }
      >
        <Show when={client.closed()}>
          {(message) => (
            <p class="netcheck-alert" role="alert">
              {message()}
            </p>
          )}
        </Show>
        <h3 class="netcheck-heading">Internet and DNS</h3>
        <ul class="netcheck-list" aria-label="Internet and DNS">
          <For each={[...items("internet"), ...items("dns")]}>{(item) => <Row item={item} />}</For>
        </ul>
        <Show when={state()?.proxy}>{(proxy) => <p class="netcheck-note">The session has the proxy {proxy()}. The checks through the proxy show if the browsers of the host reach the internet.</p>}</Show>

        <Show when={items("admin").length > 0 || fileProblems("admin").length > 0}>
          <h3 class="netcheck-heading">From the administrator</h3>
          <For each={fileProblems("admin")}>
            {(problem) => (
              <p class="netcheck-alert" role="alert">
                {problem.message}
              </p>
            )}
          </For>
          <ul class="netcheck-list" aria-label="Checks of the administrator">
            <For each={items("admin")}>{(item) => <Row item={item} />}</For>
          </ul>
          <p class="netcheck-path">
            Read-only. File: <code>{state()?.adminPath}</code>
          </p>
        </Show>

        <div class="netcheck-bar">
          <h3 class="netcheck-heading">Your checks</h3>
          <span class="netcheck-spacer" />
          <button type="button" disabled={!editable() || items("user").length === 0} aria-pressed={editMode() ? "true" : "false"} onClick={() => setEditMode(!editMode())}>
            {editMode() ? "Done" : "Edit"}
          </button>
          <button type="button" disabled={!editable()} onClick={() => startEdit(-1)}>
            Add check
          </button>
        </div>
        <Show when={state()?.error}>
          {(error) => (
            <p class="netcheck-alert" role="alert">
              The file is not valid, thus farglass does not change it. Correct the file and select Check again. {error()}
            </p>
          )}
        </Show>
        <Show when={editing() !== null}>
          <form class="netcheck-form" onSubmit={submit} aria-label={editing() === -1 ? "New check" : "Edit check"}>
            <label for="netcheck-name">Name</label>
            <input id="netcheck-name" required value={draft().name} autocomplete="off" spellcheck={false} ref={(el) => queueMicrotask(() => el.focus())} onInput={(event) => change({ name: event.currentTarget.value })} />
            <label for="netcheck-kind">Kind</label>
            <select id="netcheck-kind" value={draft().kind} onChange={(event) => change({ kind: event.currentTarget.value as NetcheckKind })}>
              <option value="tcp" selected={draft().kind === "tcp"}>
                TCP: connect to a port
              </option>
              <option value="icmp" selected={draft().kind === "icmp"}>
                ICMP: ping the host
              </option>
            </select>
            <label for="netcheck-host">Host</label>
            <input
              id="netcheck-host"
              required
              pattern="[A-Za-z0-9._:\-]+"
              title="A host name or an IP address"
              placeholder="git.example.com or 192.0.2.10"
              value={draft().host}
              autocomplete="off"
              spellcheck={false}
              onInput={(event) => change({ host: event.currentTarget.value })}
            />
            <Show when={draft().kind === "tcp"}>
              <label for="netcheck-port">Port</label>
              <input id="netcheck-port" type="number" required min="1" max="65535" placeholder="443" value={draft().port} onInput={(event) => change({ port: event.currentTarget.value })} />
            </Show>
            <label for="netcheck-timeout">Timeout (ms)</label>
            <input id="netcheck-timeout" type="number" min="100" max="10000" step="100" placeholder="2000" value={draft().timeout} onInput={(event) => change({ timeout: event.currentTarget.value })} />
            <div class="settings-buttons netcheck-form-buttons">
              <button type="button" onClick={() => setEditing(null)}>
                Cancel
              </button>
              <button type="submit">Save</button>
            </div>
          </form>
        </Show>
        <For each={fileProblems("user")}>
          {(problem) => (
            <p class="netcheck-alert" role="alert">
              {problem.message}
            </p>
          )}
        </For>
        <ul class="netcheck-list" aria-label="Your checks">
          <For each={items("user")} fallback={<li class="netcheck-empty">No checks yet. Select Add check to see if a host or a port that you need is available.</li>}>
            {(item) => <Row item={item} editable />}
          </For>
        </ul>
        <p class="netcheck-path">
          File: <code>{state()?.path}</code>
        </p>
      </Show>
      <OutcomeLine outcome={outcome()} />
      <Show when={removing() !== null}>
        <ConfirmDialog title="Delete the check?" action="Delete" onconfirm={() => remove(removing() ?? -1)} oncancel={() => setRemoving(null)}>
          <p>
            farglass removes <strong>{state()?.user[removing() ?? -1]?.name}</strong> from the file.
          </p>
        </ConfirmDialog>
      </Show>
    </section>
  );
}
