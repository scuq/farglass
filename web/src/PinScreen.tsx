import { createSignal, onCleanup, Show } from "solid-js";
import { LcarsFrame } from "./brand/LcarsFrame";
import type { Connection } from "./proto";

/**
 * The machine PIN screen (F059). The host asks for its PIN before the login screen.
 *
 * The PIN is a filter, not a factor: after it, the login screen asks for the
 * username, the password, and the passkey as before.
 */
export function PinScreen(props: { connection: Connection; onaccepted: () => void }) {
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [retryIn, setRetryIn] = createSignal(0);
  let input: HTMLInputElement | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;

  const countdown = (seconds: number) => {
    clearInterval(timer);
    setRetryIn(seconds);
    timer = setInterval(() => {
      const left = retryIn() - 1;
      setRetryIn(Math.max(left, 0));
      if (left <= 0) {
        clearInterval(timer);
        queueMicrotask(() => input?.focus());
      }
    }, 1000);
  };
  onCleanup(() => clearInterval(timer));

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    // Read the field, not a signal: a paste followed by Enter can submit before Solid commits a write.
    const pin = input?.value.trim() ?? "";
    if (!pin || busy()) return;
    setBusy(true);
    setError("");
    const result = await props.connection.sendPin(pin);
    setBusy(false);
    if (input) input.value = "";
    if (result.accepted) {
      props.onaccepted();
      return;
    }
    if (result.retryAfter > 0) {
      setError("There were too many wrong PINs from your address.");
      countdown(result.retryAfter);
    } else {
      setError("The PIN is not correct.");
      queueMicrotask(() => input?.focus());
    }
  };

  const minutes = () => Math.ceil(retryIn() / 60);

  return (
    <LcarsFrame>
      <section class="login" aria-labelledby="pin-title">
        <h1 id="pin-title">Log in to {window.location.hostname}</h1>
        <p>This host asks for its PIN before the login.</p>
        <Show when={error()}>
          <ul class="login-messages">
            <li data-kind="error" role="alert">
              {error()}
              <Show when={retryIn() > 0}> Try again in {minutes() === 1 ? "1 minute" : `${minutes()} minutes`}.</Show>
            </li>
          </ul>
        </Show>
        <form onSubmit={(event) => void submit(event)}>
          <div class="field secret">
            <label for="pin-answer">PIN of this host</label>
            <input
              id="pin-answer"
              ref={(element) => {
                input = element;
                queueMicrotask(() => element.focus());
              }}
              type="password"
              inputmode="numeric"
              pattern="[0-9]{4,6}"
              minlength={4}
              maxlength={6}
              autocomplete="off"
              disabled={busy() || retryIn() > 0}
              required
            />
          </div>
          <button type="submit" class="connect" disabled={busy() || retryIn() > 0}>
            Continue
            <span aria-hidden="true">→</span>
          </button>
        </form>
      </section>
    </LcarsFrame>
  );
}
