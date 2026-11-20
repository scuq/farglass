import { createSignal, For, onCleanup, Show } from "solid-js";
import type { Usage } from "../../../proto";
import { formatBytes, isBusy, statsClient } from "../../../stats";
import type { AppContext } from "../../types";

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

/**
 * What the desktop costs (F046): the CPU and the memory of farglassd and of the agent, from the
 * stats of the agent, and the data rate of the connection of this tab.
 */
export function ResourcesPage(props: { context: AppContext }) {
  const connection = props.context.connection;
  const [rate, setRate] = createSignal<{ received: number; sent: number } | null>(null);
  let last = { ...connection.bytes, at: performance.now() };
  const timer = setInterval(() => {
    const now = { ...connection.bytes, at: performance.now() };
    const seconds = Math.max(0.001, (now.at - last.at) / 1000);
    setRate({ received: (now.received - last.received) / seconds, sent: (now.sent - last.sent) / seconds });
    last = now;
  }, 2000);
  onCleanup(() => clearInterval(timer));

  const sample = () => statsClient()?.sample() ?? null;
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
    const each = sample();
    const traffic = rate();
    if (!each || !traffic) return null;
    return scoreOf(each.agent.cpu + (each.server?.cpu ?? 0), traffic.received + traffic.sent);
  };

  return (
    <div class="settings-page">
      <h2>Resources</h2>
      <Show when={score()} fallback={<p class="settings-note">Measuring: the first numbers come after 2 s.</p>}>
        {(level) => (
          <p class="settings-score" data-score={level()}>
            <strong>{level() === "light" ? "Light" : level() === "moderate" ? "Moderate" : "Heavy"}:</strong> {SCORE_TEXT[level()].split(": ")[1]}
          </p>
        )}
      </Show>
      <dl class="settings-facts">
        <dt>farglassd</dt>
        <dd data-field="server">{line(sample()?.server)}</dd>
        <dt>The agent of the session</dt>
        <dd data-field="agent">{line(sample()?.agent)}</dd>
        <dt>The connection of this tab</dt>
        <dd data-field="rate">{rate() ? `${formatBytes(rate()!.received)}/s in, ${formatBytes(rate()!.sent)}/s out` : "Measuring"}</dd>
      </dl>
      <h3>Apps</h3>
      <Show when={apps().length > 0} fallback={<p class="settings-note">No terminal and no Linux app is open.</p>}>
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
      </Show>
      <p class="settings-note">The numbers come every 2 s. The CPU is in percent of one core. An app is busy from 5 % CPU or 20 kB/s of network.</p>
    </div>
  );
}
