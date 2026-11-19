import { createSignal, onCleanup } from "solid-js";

/** The clock of the panel: the time of the client, which changes at each new minute. */
export function Clock(props: { now?: () => Date }) {
  const now = props.now ?? (() => new Date());
  const [time, setTime] = createSignal(now());
  const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });
  const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "full", timeStyle: "short" });
  let timer: ReturnType<typeof setTimeout> | undefined;

  const tick = () => {
    const current = now();
    setTime(current);
    // The next update at the start of the next minute.
    timer = setTimeout(tick, 60_000 - (current.getSeconds() * 1000 + current.getMilliseconds()) + 50);
  };
  timer = setTimeout(tick, 60_000 - (time().getSeconds() * 1000 + time().getMilliseconds()) + 50);
  onCleanup(() => clearTimeout(timer));

  return (
    <time class="panel-clock" datetime={time().toISOString()} title={dateFormat.format(time())}>
      {timeFormat.format(time())}
    </time>
  );
}
