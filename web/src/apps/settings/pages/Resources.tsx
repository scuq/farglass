import { createSignal, onCleanup, Show } from "solid-js";
import type { Usage } from "../../../proto";
import { formatBytes, statsClient } from "../../../stats";
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
      <p class="settings-note">The numbers come every 2 s. The CPU is in percent of one core.</p>
    </div>
  );
}
