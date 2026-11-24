import { createSignal, For, onCleanup, Show } from "solid-js";
import type { Usage } from "../../../proto";
import { formatBytes, isBusy, SAMPLE_SECONDS, type StatsPoint, statsClient } from "../../../stats";
import type { AppContext } from "../../types";
import { Section } from "../common";
import { Sparkline, type SparkSeries } from "../Sparkline";
import { niceMax } from "../sparkline";

/** The levels of the resource score. */
export type Score = "light" | "moderate" | "heavy";

/** Returns the score of the desktop: the CPU of farglassd and the agent, and the data rate of the connection in bytes each second. */
export function scoreOf(cpu: number, rate: number): Score {
  if (cpu < 10 && rate < 200_000) return "light";
  if (cpu < 50 && rate < 2_000_000) return "moderate";
  return "heavy";
}

const SCORE_TEXT: Readonly<Record<Score, string>> = {
  light: "Light: the desktop costs little.",
  moderate: "Moderate: an app draws or plays, and the desktop sends it.",
  heavy: "Heavy: a large or fast window, a video, or a slow connection. A smaller window or the video redirection helps.",
};

/** The milliseconds with no sample from which the numbers count as old: three samples. */
const STALE_MS = 3 * SAMPLE_SECONDS * 1000;

const percent = (value: number) => `${value.toFixed(1)} %`;
const rate = (value: number) => `${formatBytes(value)}/s`;

/** One number of the page with its graph of the last 5 minutes. */
function Tile(props: { label: string; field: string; value: string; graph: string; series: readonly SparkSeries[]; max: number; format: (value: number) => string }) {
  return (
    <div class="settings-tile">
      <span class="settings-tile-label">{props.label}</span>
      <span class="settings-tile-value" data-field={props.field}>
        {props.value}
      </span>
      <Sparkline label={props.graph} series={props.series} max={props.max} interval={SAMPLE_SECONDS} format={props.format} />
    </div>
  );
}

/**
 * The use of the host (F105), and what the desktop costs (F046): the CPU and the memory of
 * farglassd and of the agent, and the data rate of the connection of this tab. The numbers
 * and the graphs of the last 5 minutes come from the stats of the agent.
 */
export function ResourcesPage(props: { context: AppContext }) {
  const sample = () => statsClient()?.sample() ?? null;
  const history = () => statsClient()?.history() ?? [];
  const last = () => history().at(-1);
  /** The values of one number of the history, for a graph. */
  const values = (pick: (point: StatsPoint) => number | undefined) => history().map((point) => pick(point) ?? 0);

  // The numbers are old when the agent sends none: the page tells it and does not show them as new.
  const [age, setAge] = createSignal(0);
  const timer = setInterval(() => setAge(statsClient()?.age() ?? 0), 1000);
  onCleanup(() => clearInterval(timer));
  const stale = () => history().length > 0 && age() >= STALE_MS;

  /** The terminals and Linux apps of the desktop with their use, the most CPU first. */
  const apps = () => {
    const titles = new Map((props.context.windows?.() ?? []).map((win) => [win.id, win.title]));
    return (sample()?.windows ?? [])
      .filter((each) => titles.has(each.window))
      .map((each) => ({ id: each.window, title: titles.get(each.window)!, usage: each.usage }))
      .toSorted((a, b) => b.usage.cpu - a.usage.cpu || b.usage.memory - a.usage.memory);
  };
  const line = (usage: Usage | undefined) => (usage ? `CPU ${usage.cpu.toFixed(1)} %, memory ${formatBytes(usage.memory)}` : "Not known");
  const score = () => {
    const each = last();
    if (!each) return null;
    return scoreOf(each.agent.cpu + (each.server?.cpu ?? 0), each.rate.received + each.rate.sent);
  };
  const host = () => last()?.host;
  const received = () => values((point) => point.rate.received);
  const sent = () => values((point) => point.rate.sent);
  const hostIn = () => values((point) => point.host?.net_in);
  const hostOut = () => values((point) => point.host?.net_out);
  const serverCpu = () => values((point) => point.server?.cpu);
  const agentCpu = () => values((point) => point.agent.cpu);

  return (
    <div class="settings-page" data-stale={stale() ? "true" : undefined}>
      <h2>Resources</h2>
      <Show when={stale()}>
        <p class="settings-note settings-stale" role="status">
          The numbers are old: the agent sent none for {Math.round(age() / 1000)} s.
        </p>
      </Show>
      <Show when={host()}>
        {(use) => (
          <Section id="resources-host">
            <div class="settings-tiles">
              <Tile
                label="CPU"
                field="host-cpu"
                value={percent(use().cpu)}
                graph="CPU of the host"
                series={[{ name: "", values: values((point) => point.host?.cpu) }]}
                max={100}
                format={percent}
              />
              <Tile
                label="Memory"
                field="host-memory"
                value={`${formatBytes(use().memory)} of ${formatBytes(use().memory_total)}`}
                graph="Memory of the host"
                series={[{ name: "", values: values((point) => point.host?.memory) }]}
                max={use().memory_total}
                format={formatBytes}
              />
              <Tile
                label="Network"
                field="host-net"
                value={`${rate(use().net_in)} in, ${rate(use().net_out)} out`}
                graph="Network of the host"
                series={[
                  { name: "in", values: hostIn() },
                  { name: "out", values: hostOut() },
                ]}
                max={niceMax([...hostIn(), ...hostOut()], 10_000)}
                format={rate}
              />
            </div>
            <p class="settings-note">All users and all programs of the host. The CPU is in percent of all cores.</p>
          </Section>
        )}
      </Show>
      <Section id="resources-farglass">
        <Show when={score()} fallback={<p class="settings-note">Measuring: the first numbers come after 2 s.</p>}>
          {(level) => (
            <p class="settings-score" data-score={level()}>
              <strong>{level() === "light" ? "Light" : level() === "moderate" ? "Moderate" : "Heavy"}:</strong> {SCORE_TEXT[level()].split(": ")[1]}
            </p>
          )}
        </Show>
        <div class="settings-tiles">
          <Tile
            label="farglassd"
            field="server"
            value={line(sample()?.server)}
            graph="CPU of farglassd"
            series={[{ name: "", values: serverCpu() }]}
            max={niceMax(serverCpu(), 5)}
            format={percent}
          />
          <Tile
            label="The agent of the session"
            field="agent"
            value={line(sample()?.agent)}
            graph="CPU of the agent"
            series={[{ name: "", values: agentCpu() }]}
            max={niceMax(agentCpu(), 5)}
            format={percent}
          />
          <Tile
            label="The connection of this tab"
            field="rate"
            value={last() ? `${rate(last()!.rate.received)} in, ${rate(last()!.rate.sent)} out` : "Measuring"}
            graph="Data rate of this tab"
            series={[
              { name: "in", values: received() },
              { name: "out", values: sent() },
            ]}
            max={niceMax([...received(), ...sent()], 10_000)}
            format={rate}
          />
        </div>
        <p class="settings-note">The graphs of farglassd and of the agent show the CPU, in percent of one core.</p>
      </Section>
      <Section id="resources-apps">
        <Show when={apps().length > 0} fallback={<p class="settings-note">No terminal and no Linux app is open.</p>}>
          <div class="settings-scroll">
            <table class="settings-apps">
              <thead>
                <tr>
                  <th scope="col">Window</th>
                  <th scope="col">CPU</th>
                  <th scope="col">Memory</th>
                  <th scope="col">Network</th>
                </tr>
              </thead>
              <tbody>
                <For each={apps()}>
                  {(row) => (
                    <tr data-window={row.id} data-busy={isBusy(row.usage) ? "true" : undefined}>
                      <td>
                        <button type="button" class="settings-link" onClick={() => props.context.activateWindow?.(row.id)} title="Go to the window">
                          {row.title}
                        </button>
                      </td>
                      <td>{row.usage.cpu.toFixed(1)} %</td>
                      <td>{formatBytes(row.usage.memory)}</td>
                      <td>{row.usage.net === undefined ? "–" : `${formatBytes(row.usage.net)}/s`}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
        <p class="settings-note">The numbers come every 2 s. The CPU of an app is in percent of one core. An app is busy from 5 % CPU or 20 kB/s of network.</p>
      </Section>
    </div>
  );
}
