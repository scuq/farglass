import { createSignal, For, onCleanup, Show } from "solid-js";
import { type Channel, type Close, type Connection, connect, endpointUrl } from "./proto";

/** One line of the log of the echo panel. */
interface Line {
  readonly id: number;
  readonly text: string;
  readonly kind: "sent" | "echo" | "info" | "error";
}

/** The state of the link to the agent, as the panel shows it. */
type LinkState = "connecting" | "ready" | "refused" | "closed";

const LINK_LABEL: Record<LinkState, string> = {
  connecting: "Connecting",
  ready: "Echo channel open",
  refused: "Channel refused",
  closed: "Connection closed",
};

/**
 * A development panel: it opens an echo channel to the agent and shows the round-trip time.
 *
 * The panel proves the path from the shell to the agent and back (milestone M0).
 */
export function Echo() {
  const [state, setState] = createSignal<LinkState>("connecting");
  const [lines, setLines] = createSignal<readonly Line[]>([]);
  const [text, setText] = createSignal("");
  let nextLine = 0;
  let connection: Connection | undefined;
  let channel: Channel | undefined;
  const sentAt: number[] = [];

  const log = (kind: Line["kind"], text: string) =>
    setLines((old) => [...old.slice(-49), { id: nextLine++, text, kind }]);

  const closed = (close: Close) => {
    if (state() === "connecting") {
      setState("refused");
    } else if (state() === "ready") {
      setState("closed");
    }
    log("error", `Channel closed: ${close.reason}${close.message ? `. ${close.message}` : ""}`);
  };

  connect(endpointUrl())
    .then(async (opened) => {
      connection = opened;
      opened.onclose = (reason) => {
        setState("closed");
        log("error", `Connection closed: ${reason}`);
      };
      channel = opened.openChannel(
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
      await channel.ready;
      setState("ready");
      log("info", `Channel ${channel.id} is open.`);
    })
    .catch((error: unknown) => {
      if (state() === "connecting" && !(error && typeof error === "object" && "reason" in error)) {
        setState("closed");
        log("error", error instanceof Error ? error.message : String(error));
      }
    });

  onCleanup(() => connection?.close());

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
    <section class="echo" aria-labelledby="echo-title">
      <h1 id="echo-title">Echo channel</h1>
      <p class="echo-state" data-state={state()} role="status">
        {LINK_LABEL[state()]}
      </p>
      <form onSubmit={send}>
        <label for="echo-input">Message</label>
        <input
          id="echo-input"
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
