import { createMemo, createSignal, onCleanup, Show } from "solid-js";
import { type ClockSettings, crockfordStart, crockfordTimestamp, DEFAULT_CLOCK, formatClock, hourOptions } from "../clock";

/**
 * The clock of the panel: the time of the client, which changes at each new minute.
 * The settings select the hour format, and can add the crockford timestamp of the time (F099).
 */
export function Clock(props: { now?: () => Date; settings?: () => ClockSettings }) {
  const now = props.now ?? (() => new Date());
  const settings = () => props.settings?.() ?? DEFAULT_CLOCK;
  const [time, setTime] = createSignal(now());
  const dateFormat = createMemo(() => new Intl.DateTimeFormat(undefined, { dateStyle: "full", timeStyle: "short", ...hourOptions(settings().hours) }));
  let timer: ReturnType<typeof setTimeout> | undefined;

  const tick = () => {
    const current = now();
    setTime(current);
    // The next update at the start of the next minute.
    timer = setTimeout(tick, 60_000 - (current.getSeconds() * 1000 + current.getMilliseconds()) + 50);
  };
  timer = setTimeout(tick, 60_000 - (time().getSeconds() * 1000 + time().getMilliseconds()) + 50);
  onCleanup(() => clearTimeout(timer));

  const code = () => (settings().crockford ? crockfordTimestamp(time()) : null);

  return (
    <>
      <time class="panel-clock" datetime={time().toISOString()} title={dateFormat().format(time())}>
        {formatClock(time(), settings().hours)}
      </time>
      <Show when={code()}>
        <time class="panel-crockford" datetime={time().toISOString()} title={`Crockford timestamp of ${crockfordStart(time())} UTC`}>
          {code()}
        </time>
      </Show>
    </>
  );
}
