import { createSignal, createUniqueId, For, onCleanup, Show } from "solid-js";
import type { Connection, PolkitEvent, PolkitReply } from "../proto";
import "./polkit.css";

/** One request of polkit, as the dialog shows it. */
interface Request {
  readonly id: number;
  readonly action: string;
  readonly message: string;
  readonly identities: readonly string[];
  readonly identity: string;
}

/** The state of the conversation of the shown request. */
interface Conversation {
  readonly prompt: { readonly text: string; readonly echo: boolean } | null;
  readonly info: readonly string[];
  readonly failed: boolean;
  readonly checking: boolean;
}

const EMPTY: Conversation = { prompt: null, info: [], failed: false, checking: false };

/** Reports whether the browser has modal dialogs. A test DOM can have none. */
const MODAL = typeof HTMLDialogElement !== "undefined" && "showModal" in HTMLDialogElement.prototype;

/**
 * The polkit dialog of the desktop. It keeps a polkit channel open: while it is
 * open, programs of the session can ask for an authentication, and the dialog
 * shows the PAM conversation. With no open channel, the agent cancels each request.
 */
export function PolkitDialog(props: { connection: Connection }) {
  const [queue, setQueue] = createSignal<readonly Request[]>([]);
  const shown = () => queue()[0];
  const [talk, setTalk] = createSignal<Conversation>(EMPTY);
  const [identity, setIdentity] = createSignal("");
  const titleId = createUniqueId();
  const answerId = createUniqueId();
  let dialog: HTMLDialogElement | undefined;
  let input: HTMLInputElement | undefined;

  // The logic uses plain variables: Solid commits a signal write on the next microtask,
  // and events can come in the same task, for example a begin and its first prompt.
  // The signals are copies for the view.
  let requests: Request[] = [];
  let conversation: Conversation = EMPTY;
  let chosen = "";
  const update = (next: Conversation) => {
    conversation = next;
    setTalk(next);
  };
  const choose = (name: string) => {
    chosen = name;
    setIdentity(name);
  };
  const current = () => requests[0];
  const send = (reply: PolkitReply) => channel.control(reply);

  const show = () => {
    queueMicrotask(() => {
      if (MODAL && dialog && current() && !dialog.open) {
        dialog.showModal();
      }
      input?.focus();
    });
  };

  const finish = (id: number) => {
    requests = requests.filter((request) => request.id !== id);
    setQueue(requests);
    update(EMPTY);
    const next = requests[0];
    if (next) {
      choose(next.identity);
      show();
    } else if (MODAL && dialog?.open) {
      dialog.close();
    }
  };

  const onEvent = (event: PolkitEvent) => {
    switch (event.type) {
      case "begin":
        requests = [
          ...requests,
          { id: event.request, action: event.action, message: event.message, identities: event.identities, identity: event.identity },
        ];
        setQueue(requests);
        if (requests.length === 1) {
          choose(event.identity);
          show();
        }
        return;
      case "done":
        finish(event.request);
        return;
    }
    if (event.request !== current()?.id) {
      return;
    }
    switch (event.type) {
      case "prompt":
        update({ ...conversation, prompt: { text: event.text, echo: event.echo }, checking: false });
        queueMicrotask(() => input?.focus());
        break;
      case "info":
        update({ ...conversation, info: [...conversation.info, event.text] });
        break;
      case "failed":
        // The password was not correct. A new attempt starts at once.
        update({ ...EMPTY, failed: true });
        send({ type: "start", request: event.request, identity: chosen });
        break;
    }
  };

  const channel = props.connection.openChannel({ kind: "polkit" }, { oncontrol: (message) => onEvent(message as PolkitEvent) });
  onCleanup(() => channel.close());

  const submit = (event: SubmitEvent) => {
    event.preventDefault();
    const request = current();
    if (!request || !conversation.prompt) {
      return;
    }
    const text = input?.value ?? "";
    if (input) {
      input.value = "";
    }
    update({ ...conversation, prompt: null, checking: true, failed: false });
    send({ type: "answer", request: request.id, text });
  };

  const cancel = () => {
    const request = current();
    if (request) {
      send({ type: "cancel", request: request.id });
    }
  };

  const changeIdentity = (name: string) => {
    const request = current();
    if (request && name !== chosen) {
      choose(name);
      update(EMPTY);
      send({ type: "start", request: request.id, identity: name });
    }
  };

  return (
    <dialog
      class="polkit-dialog"
      aria-labelledby={titleId}
      hidden={!MODAL && !shown()}
      ref={(el) => {
        dialog = el;
        // Escape gives the cancel event of the dialog.
        el.addEventListener("cancel", (event) => {
          event.preventDefault();
          cancel();
        });
      }}
    >
      <Show when={shown()}>
        {(request) => (
          <form onSubmit={submit}>
            <h2 id={titleId}>Authentication is necessary</h2>
            <p class="polkit-message">{request().message}</p>
            <p class="polkit-action">{request().action}</p>
            <Show
              when={request().identities.length > 1}
              fallback={<p>Authenticate as {identity()}</p>}
            >
              <label>
                Authenticate as{" "}
                <select value={identity()} onChange={(event) => changeIdentity(event.currentTarget.value)}>
                  <For each={request().identities}>{(name) => <option value={name}>{name}</option>}</For>
                </select>
              </label>
            </Show>
            <For each={talk().info}>{(line) => <p class="polkit-info">{line}</p>}</For>
            <Show when={talk().failed}>
              <p class="polkit-failed" role="alert">
                The authentication failed. Try again.
              </p>
            </Show>
            <label for={answerId}>{talk().prompt?.text.trim().replace(/:$/, "") || "Password"}</label>
            <input
              id={answerId}
              ref={(el) => {
                input = el;
              }}
              type={talk().prompt?.echo ? "text" : "password"}
              autocomplete={talk().prompt?.echo ? "username" : "current-password"}
              disabled={!talk().prompt}
            />
            <p class="polkit-state" role="status">
              {talk().checking ? "Checking" : talk().prompt ? "" : "Waiting for the system"}
            </p>
            <div class="polkit-buttons">
              <button type="button" onClick={cancel}>
                Cancel
              </button>
              <button type="submit" class="primary" disabled={!talk().prompt}>
                Authenticate
              </button>
            </div>
          </form>
        )}
      </Show>
    </dialog>
  );
}
