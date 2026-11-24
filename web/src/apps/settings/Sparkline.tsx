import { createMemo, createSignal, For, Show } from "solid-js";
import { HISTORY_SLOTS, sparkAge, sparkIndex, sparkPaths, sparkSpan, sparkX } from "./sparkline";

const WIDTH = 300;
const HEIGHT = 44;

/** One line of a graph. The first series has an area below its line, a second one is a dashed line. */
export interface SparkSeries {
  /** The name of the series, for the key and the readout. Empty for a graph with one series. */
  readonly name: string;
  /** The values, the oldest first. */
  readonly values: readonly number[];
}

/**
 * A graph of the last 5 minutes (F105). Its colors are CSS variables, thus it follows the
 * theme and the accent color. Below the graph is the time that it shows and the peak, or
 * the value below the pointer.
 */
export function Sparkline(props: {
  /** What the graph shows, for a screen reader: "CPU of the host". */
  label: string;
  series: readonly SparkSeries[];
  /** The top of the scale. */
  max: number;
  /** The seconds between two values. */
  interval: number;
  format: (value: number) => string;
}) {
  const [hover, setHover] = createSignal<number | null>(null);
  const count = () => props.series[0]?.values.length ?? 0;
  const shown = (values: readonly number[]) => values.slice(-HISTORY_SLOTS);
  const paths = createMemo(() => props.series.map((each) => sparkPaths(each.values, props.max, HISTORY_SLOTS, WIDTH, HEIGHT)));
  /** The positions of the series. The number of series of a graph does not change. */
  const positions = createMemo(() => props.series.map((_, at) => at));
  const peak = (values: readonly number[]) => (values.length > 0 ? Math.max(...shown(values)) : 0);
  const named = (each: SparkSeries, value: number) => `${each.name ? `${each.name} ` : ""}${props.format(value)}`;
  /** The index of the value below the pointer, in the values that show. */
  const index = () => {
    const at = hover();
    return at === null ? null : sparkIndex(at * WIDTH, Math.min(count(), HISTORY_SLOTS), HISTORY_SLOTS, WIDTH);
  };
  const caption = () => {
    const at = index();
    if (count() < 2) return "The graph starts with the second sample";
    if (at === null) return `${sparkSpan((Math.min(count(), HISTORY_SLOTS) - 1) * props.interval)}, peak ${props.series.map((each) => named(each, peak(each.values))).join(", ")}`;
    const total = Math.min(count(), HISTORY_SLOTS);
    return `${sparkAge(at, total, props.interval)}: ${props.series.map((each) => named(each, shown(each.values)[at] ?? 0)).join(", ")}`;
  };
  const summary = () => {
    const last = props.series.map((each) => named(each, each.values.at(-1) ?? 0)).join(", ");
    return `${props.label}, last 5 minutes. Now ${last}. Peak ${props.series.map((each) => named(each, peak(each.values))).join(", ")}.`;
  };
  const move = (event: PointerEvent) => {
    const box = (event.currentTarget as Element).getBoundingClientRect();
    setHover(box.width > 0 ? Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)) : null);
  };

  return (
    <figure class="spark">
      <svg
        class="spark-plot"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        role="img"
        aria-label={summary()}
        onPointerMove={move}
        onPointerLeave={() => setHover(null)}
      >
        <line class="spark-grid" x1="0" x2={WIDTH} y1={HEIGHT / 2} y2={HEIGHT / 2} />
        <line class="spark-grid" x1="0" x2={WIDTH} y1={HEIGHT - 0.5} y2={HEIGHT - 0.5} />
        <path class="spark-area" d={paths()[0]?.area ?? ""} />
        <For each={positions()}>{(at) => <path class="spark-line" data-series={at} d={paths()[at]?.line ?? ""} />}</For>
        <Show when={index() !== null}>
          <line
            class="spark-cursor"
            x1={sparkX(index()!, Math.min(count(), HISTORY_SLOTS), HISTORY_SLOTS, WIDTH)}
            x2={sparkX(index()!, Math.min(count(), HISTORY_SLOTS), HISTORY_SLOTS, WIDTH)}
            y1="0"
            y2={HEIGHT}
          />
        </Show>
      </svg>
      <figcaption class="spark-caption">
        <span>{caption()}</span>
        <Show when={props.series.length > 1}>
          <span class="spark-keys" aria-hidden="true">
            <For each={positions()}>
              {(at) => (
                <span class="spark-key" data-series={at}>
                  {props.series[at]?.name}
                </span>
              )}
            </For>
          </span>
        </Show>
      </figcaption>
    </figure>
  );
}
