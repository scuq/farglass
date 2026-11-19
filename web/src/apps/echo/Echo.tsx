import { createSignal, createUniqueId, For, onCleanup, Show } from "solid-js";
import type { Channel, Close } from "../../proto";
import type { AppContext } from "../types";

/** One line of the log of the echo panel. */
interface Line {
  readonly id: number;
  readonly text: string;
  readonly kind: "sent" | "echo" | "info" | "error";
}

/** The state of the link to the agent, as the panel shows it. */
type LinkState = "connecting" | "ready" | "refused" | "closed";

const LINK_LABEL: Record<LinkState, string> = {
  connecting: "Opening the echo channel",
  ready: "Echo channel open",
  refused: "Channel refused",
  closed: "Connection closed",
};

/**
 * A development panel: it opens an echo channel to the agent and shows the round-trip time.
 *
 * The panel proves the path from the shell to the agent and back (milestone M0).
 */
export default function Echo(props: { context: AppContext }) {
  const [state, setState] = createSignal<LinkState>("connecting");
  const [lines, setLines] = createSignal<readonly Line[]>([]);
  const [text, setText] = createSignal("");
  let nextLine = 0;
  let channel: Channel | undefined;
  const sentAt: number[] = [];

  // The log is in the state store of the agent under the key of the window: a reload shows the same lines.
  const store = props.context.state;
  const logKey = store.key("log");
  // Each window has its own panel, thus the ids of the elements must differ.
  const inputId = createUniqueId();
  const titleId = createUniqueId();
  const log = (kind: Line["kind"], text: string) => {
    const next = [...lines().slice(-49), { id: nextLine++, text, kind }];
    setLines(next);
    if (kind === "sent" || kind === "echo") {
      store.set(logKey, next.filter((line) => line.kind === "sent" || line.kind === "echo"));
    }
  };
  store.watch(logKey, (value: unknown) => {
    if (Array.isArray(value)) {
      const restored = value as Line[];
      nextLine = Math.max(nextLine, ...restored.map((line) => line.id + 1));
      setLines(restored);
    }
  });

  const closed = (close: Close) => {
    if (state() === "connecting") {
      setState("refused");
    } else if (state() === "ready") {
      setState("closed");
    }
    log("error", `Channel closed: ${close.reason}${close.message ? `. ${close.message}` : ""}`);
  };

  channel = props.context.openChannel(
    { kind: "echo" },
    {
      ondata: (bytes) => {
        const start = sentAt.shift();
        const time = start === undefined ? "" : ` (${(performance.now() - start).toFixed(1)} ms)`;
        log("echo", `${new TextDecoder().decode(bytes)}${time}`);
      },
      onclose: closed,
    },
  );
  const opened = channel;
  opened.ready.then(
    () => {
      setState("ready");
      log("info", `Channel ${opened.id} is open.`);
    },
    // The close callback shows the cause.
    () => {},
  );

  onCleanup(() => channel?.close());

  const send = (event: SubmitEvent) => {
    event.preventDefault();
    const message = text();
    if (!channel || state() !== "ready" || message === "") {
      return;
    }
    sentAt.push(performance.now());
    channel.send(new TextEncoder().encode(message));
    log("sent", message);
    setText("");
  };

  return (
    <section class="echo" aria-labelledby={titleId}>
      <h1 id={titleId}>Echo channel</h1>
      <p class="echo-state" data-state={state()} role="status">
        {LINK_LABEL[state()]}
      </p>
      <form onSubmit={send}>
        <label for={inputId}>Message</label>
        <input
          id={inputId}
          value={text()}
          onInput={(event) => setText(event.currentTarget.value)}
          disabled={state() !== "ready"}
          autocomplete="off"
        />
        <button type="submit" disabled={state() !== "ready" || text() === ""}>
          Send
        </button>
      </form>
      <Show when={lines().length > 0}>
        <ol class="echo-log" aria-live="polite">
          <For each={lines()}>{(line) => <li data-kind={line.kind}>{line.text}</li>}</For>
        </ol>
      </Show>
    </section>
  );
}
