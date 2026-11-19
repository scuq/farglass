import { createSignal, For, onCleanup, Show } from "solid-js";
import { describeSession, Login, promptAutocomplete, promptLabel } from "./login";
import type { Connection, LocalSession, LoginEvent } from "./proto";

/** The prompt that waits for an answer. */
interface Prompt {
  readonly prompt: number;
  readonly text: string;
  readonly echo: boolean;
}

/** A message of a PAM module or of the login screen. */
interface Message {
  readonly id: number;
  readonly kind: "info" | "error";
  readonly text: string;
}

/** The state of the login screen. */
type Stage = "waiting" | "prompt" | "checking" | "confirm" | "failed" | "blocked";

/**
 * The login screen. It shows each PAM prompt and message from farglassd.
 *
 * The screen asks only what the PAM modules ask, for example a username,
 * a password, and then a one-time code.
 */
export function LoginScreen(props: { connection: Connection; onlogin: (user: string) => void }) {
  const [stage, setStage] = createSignal<Stage>("waiting");
  const [prompt, setPrompt] = createSignal<Prompt | null>(null);
  const [messages, setMessages] = createSignal<readonly Message[]>([]);
  const [answer, setAnswer] = createSignal("");
  const [retryIn, setRetryIn] = createSignal(0);
  const [local, setLocal] = createSignal<readonly LocalSession[]>([]);
  let nextMessage = 0;
  let login: Login | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  let input: HTMLInputElement | undefined;
  let cancelButton: HTMLButtonElement | undefined;

  const say = (kind: Message["kind"], text: string) =>
    setMessages((old) => [...old, { id: nextMessage++, kind, text }]);

  const onevent = (event: LoginEvent) => {
    switch (event.type) {
      case "prompt":
        setPrompt({ prompt: event.prompt, text: event.text, echo: event.echo });
        setAnswer("");
        setStage("prompt");
        queueMicrotask(() => input?.focus());
        break;
      case "info":
        say("info", event.text);
        break;
      case "error":
        say("error", event.text);
        break;
      case "confirm":
        setPrompt(null);
        setLocal(event.sessions);
        setStage("confirm");
        queueMicrotask(() => cancelButton?.focus());
        break;
      case "success":
        props.onlogin(event.user);
        break;
      case "failure":
        setPrompt(null);
        say("error", event.message);
        setStage("failed");
        break;
      case "blocked":
        setPrompt(null);
        setStage("blocked");
        countdown(event.retry_after);
        break;
    }
  };

  const countdown = (seconds: number) => {
    clearInterval(timer);
    setRetryIn(seconds);
    timer = setInterval(() => {
      const left = retryIn() - 1;
      setRetryIn(Math.max(left, 0));
      if (left <= 0) {
        clearInterval(timer);
      }
    }, 1000);
  };

  /** Starts an attempt. The first attempt starts during the render, thus it changes no state. */
  const begin = () => {
    login = new Login(props.connection, {
      onevent,
      onstop: (close) => {
        setPrompt(null);
        say("error", close.message ?? "The login stopped.");
        setStage("failed");
      },
    });
  };

  const submit = (event: SubmitEvent) => {
    event.preventDefault();
    const current = prompt();
    if (!login || !current) {
      return;
    }
    // Read the field, not the signal: Solid commits a write on the next microtask,
    // and a paste followed by Enter can submit before that.
    login.answer(current.prompt, input?.value ?? answer());
    if (input) {
      input.value = "";
    }
    setAnswer("");
    setStage("checking");
  };

  const confirm = (accept: boolean) => {
    login?.confirm(accept);
    setStage("checking");
  };

  /** Starts a new attempt after a failure or a block. */
  const retry = () => {
    setMessages([]);
    setPrompt(null);
    setStage("waiting");
    begin();
  };

  begin();
  onCleanup(() => {
    clearInterval(timer);
    login?.cancel();
  });

  return (
    <section class="login" aria-labelledby="login-title">
      <h1 id="login-title">Log in to {window.location.hostname}</h1>

      <Show when={messages().length > 0}>
        <ul class="login-messages">
          <For each={messages()}>
            {(message) => (
              <li data-kind={message.kind} role={message.kind === "error" ? "alert" : undefined}>
                {message.text}
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show when={prompt()}>
        {(current) => (
          <form onSubmit={submit}>
            <label for="login-answer">{promptLabel(current().text)}</label>
            <input
              id="login-answer"
              ref={(element) => {
                input = element;
              }}
              type={current().echo ? "text" : "password"}
              autocomplete={promptAutocomplete(current().text, current().echo)}
              autocapitalize="off"
              spellcheck={false}
              value={answer()}
              onInput={(event) => setAnswer(event.currentTarget.value)}
              disabled={stage() !== "prompt"}
              required
            />
            <button type="submit" disabled={stage() !== "prompt"}>
              Continue
            </button>
          </form>
        )}
      </Show>

      <Show when={stage() === "confirm"}>
        <div class="login-confirm" role="alertdialog" aria-labelledby="confirm-title" aria-describedby="confirm-text">
          <h2 id="confirm-title">End the local session?</h2>
          <p id="confirm-text">
            You have {local().map(describeSession).join(" and ")} on this host. One user has one graphical session at a
            time. If you continue, the local session ends, and work that is not saved in it is lost.
          </p>
          <div class="login-confirm-actions">
            <button
              type="button"
              ref={(element) => {
                cancelButton = element;
              }}
              onClick={() => confirm(false)}
            >
              Cancel
            </button>
            <button type="button" class="danger" onClick={() => confirm(true)}>
              End the local session and log in
            </button>
          </div>
        </div>
      </Show>

      <p class="login-state" role="status">
        {stage() === "waiting"
          ? "Waiting for the login service"
          : stage() === "checking"
            ? "Checking"
            : stage() === "blocked"
              ? retryIn() > 0
                ? `Too many failed logins. Try again in ${retryIn()} s.`
                : "You can try again now."
              : ""}
      </p>

      <Show when={stage() === "failed" || stage() === "blocked"}>
        <button type="button" onClick={retry} disabled={stage() === "blocked" && retryIn() > 0}>
          Try again
        </button>
      </Show>
    </section>
  );
}
