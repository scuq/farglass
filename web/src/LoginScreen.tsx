import { createSignal, For, onCleanup, Show } from "solid-js";
import { LcarsFrame } from "./brand/LcarsFrame";
import { describeSession, Login, promptAutocomplete, promptLabel } from "./login";
import { credentialJson, hasPasskeys, passkeyError, requestOptions } from "./passkey";
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
type Stage = "waiting" | "prompt" | "checking" | "confirm" | "passkey" | "failed" | "blocked";

/** The passkey step of a login (F034). */
interface PasskeyStep {
  /** The options of `navigator.credentials.get`, or null if no passkey belongs to this host name. */
  readonly options: unknown;
  /** The host names of the other passkeys of the user. */
  readonly elsewhere: readonly string[];
}

/**
 * The login screen. It shows each PAM prompt and message from farglassd.
 *
 * The screen asks only what the PAM modules ask, for example a username,
 * a password, and then a one-time code.
 *
 * With `locked`, it is the lock screen: farglassd starts the login for that
 * user only, thus PAM does not ask for the username.
 */
export function LoginScreen(props: {
  connection: Connection;
  onlogin: (user: string, ticket?: string) => void;
  locked?: string | null;
  onswitch?: () => void;
}) {
  const [stage, setStage] = createSignal<Stage>("waiting");
  const [prompt, setPrompt] = createSignal<Prompt | null>(null);
  const [messages, setMessages] = createSignal<readonly Message[]>([]);
  const [answer, setAnswer] = createSignal("");
  const [retryIn, setRetryIn] = createSignal(0);
  const [local, setLocal] = createSignal<readonly LocalSession[]>([]);
  const [passkey, setPasskey] = createSignal<PasskeyStep | null>(null);
  const [useCode, setUseCode] = createSignal(false);
  let codeInput: HTMLInputElement | undefined;
  let passkeyButton: HTMLButtonElement | undefined;
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
      case "passkey": {
        setPrompt(null);
        const options = event.options ?? null;
        setPasskey({ options, elsewhere: event.elsewhere });
        // With no passkey for this host name, or no WebAuthn in this browser, only a recovery code works.
        setUseCode(options === null || !hasPasskeys());
        setStage("passkey");
        queueMicrotask(() => (useCode() ? codeInput?.focus() : passkeyButton?.focus()));
        break;
      }
      case "verified":
        break;
      case "success":
        props.onlogin(event.user, event.ticket);
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

  /** Asks the browser for the passkey and gives it to farglassd. A failed request lets the user try again or use a code. */
  const usePasskey = async () => {
    const step = passkey();
    if (!login || !step?.options) return;
    try {
      const credential = (await navigator.credentials.get({ publicKey: requestOptions(step.options) })) as PublicKeyCredential | null;
      if (!credential) throw new DOMException("no credential", "NotAllowedError");
      login.passkey(credentialJson(credential));
      setStage("checking");
    } catch (error) {
      say("error", passkeyError(error));
    }
  };

  const submitCode = (event: SubmitEvent) => {
    event.preventDefault();
    const code = codeInput?.value.trim() ?? "";
    if (!login || !code) return;
    login.recoveryCode(code);
    if (codeInput) codeInput.value = "";
    setStage("checking");
  };

  /** Starts a new attempt after a failure or a block. */
  const retry = () => {
    setMessages([]);
    setPrompt(null);
    setPasskey(null);
    setStage("waiting");
    begin();
  };

  begin();
  onCleanup(() => {
    clearInterval(timer);
    login?.cancel();
  });

  return (
    <LcarsFrame>
      <section class="login" data-locked={props.locked ? "true" : undefined} aria-labelledby="login-title">
        <Show when={props.locked} fallback={<h1 id="login-title">Log in to {window.location.hostname}</h1>}>
          {(user) => (
            <>
              <h1 id="login-title">Locked</h1>
              <p class="login-locked">
                The session of <strong>{user()}</strong> on {window.location.hostname} is locked. Your programs continue.
              </p>
            </>
          )}
        </Show>

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
              <div class={current().echo ? "field" : "field secret"}>
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
              </div>
              <button type="submit" class="connect" disabled={stage() !== "prompt"}>
                Continue
                <span aria-hidden="true">→</span>
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
                class="login-secondary"
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

        <Show when={stage() === "passkey" && passkey()}>
          {(step) => (
            <div class="login-passkey">
              <h2>Second step: your passkey</h2>
              <Show when={step().elsewhere.length > 0 && !step().options}>
                <p>
                  Your passkeys belong to {step().elsewhere.join(" and ")}. A passkey works only on its host name: open farglass at{" "}
                  {step().elsewhere.map((host) => `https://${host}`).join(" or ")}, or use a recovery code.
                </p>
              </Show>
              <Show when={step().options && !hasPasskeys()}>
                <p>This browser cannot use a passkey on this page. A passkey needs HTTPS with a valid certificate. Use a recovery code.</p>
              </Show>
              <Show when={step().options && hasPasskeys()}>
                <p>Use the passkey that you added in Settings &gt; Passkeys: your phone, a security key, or this computer.</p>
                <button
                  type="button"
                  class="connect"
                  ref={(element) => {
                    passkeyButton = element;
                  }}
                  onClick={() => void usePasskey()}
                >
                  Use my passkey
                  <span aria-hidden="true">→</span>
                </button>
              </Show>
              <Show
                when={useCode()}
                fallback={
                  <button type="button" class="login-link" onClick={() => setUseCode(true)}>
                    I lost my passkey: use a recovery code
                  </button>
                }
              >
                <form onSubmit={submitCode}>
                  <div class="field secret">
                    <label for="login-code">Recovery code</label>
                    <input
                      id="login-code"
                      ref={(element) => {
                        codeInput = element;
                      }}
                      type="text"
                      autocomplete="one-time-code"
                      autocapitalize="off"
                      spellcheck={false}
                      placeholder="xxxx-xxxx-xxxx-xxxx"
                      required
                    />
                  </div>
                  <button type="submit" class="connect">
                    Continue
                    <span aria-hidden="true">→</span>
                  </button>
                </form>
                <p class="login-hint">Each recovery code works one time. With no code left, the administrator can remove your passkeys.</p>
              </Show>
            </div>
          )}
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
          <button type="button" class="login-secondary" onClick={retry} disabled={stage() === "blocked" && retryIn() > 0}>
            Try again
          </button>
        </Show>

        <Show when={props.locked && props.onswitch}>
          <button type="button" class="login-switch" onClick={() => props.onswitch?.()}>
            Log in as a different user
          </button>
        </Show>
      </section>
    </LcarsFrame>
  );
}
