import { createSignal, createUniqueId, For, onCleanup, Show } from "solid-js";
import type { DbusClient } from "../../../dbus";
import { CLOCK_KEY, type ClockSettings, DEFAULT_CLOCK, formatClock, parseClockSettings } from "../../../clock";
import type { AppContext } from "../../types";
import { Loading, type Outcome, OutcomeLine, runAction, Section } from "../common";

const DEST = "org.freedesktop.timedate1";
const PATH = "/org/freedesktop/timedate1";

interface TimeState {
  readonly timezone: string;
  readonly ntp: boolean;
  readonly canNtp: boolean;
  readonly synchronized: boolean;
  /** The time of the host in milliseconds, at `readAt` of performance.now(). */
  readonly hostMs: number;
  readonly readAt: number;
}

/** Returns the time of the host as text in its timezone. */
export function formatHostTime(ms: number, timezone: string): string {
  try {
    return new Intl.DateTimeFormat(undefined, { dateStyle: "full", timeStyle: "medium", timeZone: timezone }).format(ms);
  } catch {
    return new Date(ms).toISOString();
  }
}

/** The hour formats of the clock in the panel (F099). */
const HOUR_CHOICES: readonly { id: ClockSettings["hours"]; label: string }[] = [
  { id: "auto", label: "Same as the language of the browser" },
  { id: "24", label: "24 hours" },
  { id: "12", label: "12 hours" },
];

/** The clock of the panel, and the time, the timezone, and the network time of the host, from timedate1. */
export function DateTimePage(props: { dbus: DbusClient; context: AppContext }) {
  // Each window of Settings has its own radio group and its own list of timezones.
  const group = createUniqueId();
  const [clock, setClock] = createSignal(DEFAULT_CLOCK);
  props.context.state.watchShared(CLOCK_KEY, (stored) => setClock(parseClockSettings(stored)));
  const changeClock = (patch: Partial<ClockSettings>) => {
    const next = { ...clock(), ...patch };
    setClock(next);
    props.context.state.setShared(CLOCK_KEY, next);
  };
  // An example time for each choice: 14:05, or 2:05 PM.
  const example = new Date(2026, 5, 29, 14, 5);
  const [state, setState] = createSignal<TimeState | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [zones, setZones] = createSignal<readonly string[]>([]);
  const [now, setNow] = createSignal(performance.now());
  const [outcome, setOutcome] = createSignal<Outcome>(null);
  let zoneInput: HTMLInputElement | undefined;

  const load = () =>
    props.dbus.getAllProperties(DEST, PATH, DEST).then(
      (p) =>
        setState({
          timezone: String(p.Timezone ?? ""),
          ntp: p.NTP === true,
          canNtp: p.CanNTP === true,
          synchronized: p.NTPSynchronized === true,
          hostMs: Number(p.TimeUSec ?? 0) / 1000,
          readAt: performance.now(),
        }),
      () => setError("The time service of the host is not available."),
    );
  void load();
  props.dbus.call(DEST, PATH, DEST, "ListTimezones").then(
    ([list]) => setZones(list as string[]),
    () => setZones([]),
  );
  const timer = setInterval(() => setNow(performance.now()), 1000);
  let unsubscribe: (() => void) | undefined;
  let closed = false;
  props.dbus
    .subscribe({ sender: DEST, path: PATH, interface: "org.freedesktop.DBus.Properties", member: "PropertiesChanged" }, () => void load())
    .then(
      (stop) => (closed ? stop() : (unsubscribe = stop)),
      () => {},
    );
  onCleanup(() => {
    closed = true;
    clearInterval(timer);
    unsubscribe?.();
  });

  const hostTime = (current: TimeState) => formatHostTime(current.hostMs + (now() - current.readAt), current.timezone);

  const setTimezone = (event: SubmitEvent) => {
    event.preventDefault();
    const zone = zoneInput?.value.trim() ?? "";
    if (zones().length > 0 && !zones().includes(zone)) {
      setOutcome({ kind: "error", text: `${zone || "An empty name"} is not a timezone of this host.` });
      return;
    }
    void runAction(() => props.dbus.call(DEST, PATH, DEST, "SetTimezone", "sb", [zone, true], { interactive: true }), setOutcome, `The timezone is ${zone}.`).then(load);
  };
  const setNtp = (on: boolean) =>
    void runAction(
      () => props.dbus.call(DEST, PATH, DEST, "SetNTP", "bb", [on, true], { interactive: true }),
      setOutcome,
      on ? "The host gets its time from the network." : "The network time is off.",
    ).then(load);

  return (
    <div class="settings-page">
      <h2>Date and time</h2>
      <Section id="datetime-clock" group>
        <div class="settings-choices">
          <For each={HOUR_CHOICES}>
            {(choice) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-clock-hours`} value={choice.id} checked={clock().hours === choice.id} onChange={() => changeClock({ hours: choice.id })} />
                <span>
                  {choice.label} <span class="settings-muted">({formatClock(example, choice.id)})</span>
                </span>
              </label>
            )}
          </For>
        </div>
        <div class="settings-choices settings-sep">
          <label class="settings-choice">
            <input type="checkbox" name="clock-crockford" checked={clock().crockford} onChange={(event) => changeClock({ crockford: event.currentTarget.checked })} />
            Also show the time as a crockford timestamp
          </label>
          <p class="settings-note">
            A crockford timestamp is a date and a time in UTC in five characters, for example 66X82 for 29 June 2026 08:04 UTC. It changes every 2 minutes.{" "}
            <a href="https://scuq.github.io/crockford-timestamp/" target="_blank" rel="noopener noreferrer">
              About the format
            </a>
          </p>
        </div>
      </Section>
      <Section id="datetime-host">
        <Loading when={state()} error={error()}>
          {(current) => (
            <>
              <dl class="settings-facts">
                <dt>Time of the host</dt>
                <dd data-field="host-time">{hostTime(current())}</dd>
                <dt>Network time</dt>
                <dd>{current().ntp ? (current().synchronized ? "On, synchronized" : "On, not synchronized yet") : "Off"}</dd>
              </dl>
              <div class="settings-fields settings-sep">
                <form class="settings-row" onSubmit={setTimezone}>
                  <label for="settings-timezone">Timezone</label>
                  <input
                    id="settings-timezone"
                    list={`${group}-timezones`}
                    value={current().timezone}
                    autocomplete="off"
                    spellcheck={false}
                    ref={(el) => {
                      zoneInput = el;
                    }}
                  />
                  <datalist id={`${group}-timezones`}>
                    <For each={zones()}>{(zone) => <option value={zone} />}</For>
                  </datalist>
                  <button type="submit">Set timezone</button>
                </form>
              </div>
              <Show when={current().canNtp}>
                <label class="settings-choice">
                  <input type="checkbox" checked={current().ntp} onChange={(event) => setNtp(event.currentTarget.checked)} />
                  Get the time from the network
                </label>
              </Show>
            </>
          )}
        </Loading>
      </Section>
      <OutcomeLine outcome={outcome()} />
    </div>
  );
}
