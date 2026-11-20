import { createSignal, For, onCleanup, Show } from "solid-js";
import { creationOptions, credentialJson, hasPasskeys, passkeyError, requestOptions } from "../../../passkey";
import type { PasskeyEvent, PasskeyInfo, PasskeyRequest } from "../../../proto";
import type { AppContext } from "../../types";

const date = (seconds: number) => new Date(seconds * 1000).toLocaleDateString(undefined, { dateStyle: "medium" });

/**
 * The passkeys of the user (F034). With a passkey, each login asks for it after the password.
 * farglassd keeps the passkeys: this page talks to it on a passkeys channel, not to the agent.
 */
export function PasskeysPage(props: { context: AppContext }) {
  const [passkeys, setPasskeys] = createSignal<readonly PasskeyInfo[] | null>(null);
  const [codesLeft, setCodesLeft] = createSignal(0);
  const [host, setHost] = createSignal(window.location.hostname);
  const [codes, setCodes] = createSignal<readonly string[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [name, setName] = createSignal("");
  const [closed, setClosed] = createSignal<string | null>(null);
  /** A change waits for a fresh passkey or a recovery code (F053). */
  const [confirm, setConfirm] = createSignal<{ options: unknown; elsewhere: readonly string[] } | null>(null);
  /** The first passkey waits for the password (F061). */
  const [askPassword, setAskPassword] = createSignal(false);
  let passwordInput: HTMLInputElement | undefined;
  let codeInput: HTMLInputElement | undefined;

  const channel = props.context.connection.openChannel(
    { kind: "passkeys" },
    {
      oncontrol: (message) => void onevent(message as PasskeyEvent),
      onclose: (close) => setClosed(close.message ?? "The passkeys are not available."),
    },
  );
  onCleanup(() => channel.close({ reason: "done" }));
  const send = (request: PasskeyRequest) => channel.control(request);
  channel.ready.then(() => send({ type: "list" })).catch(() => {});

  const onevent = async (event: PasskeyEvent) => {
    switch (event.type) {
      case "list":
        setPasskeys(event.passkeys);
        setCodesLeft(event.codes);
        setHost(event.host);
        setBusy(false);
        break;
      case "codes":
        setCodes(event.codes);
        break;
      case "error":
        setError(event.message);
        setBusy(false);
        break;
      case "confirm":
        setConfirm({ options: event.options ?? null, elsewhere: event.elsewhere });
        break;
      case "password":
        setAskPassword(true);
        queueMicrotask(() => passwordInput?.focus());
        break;
      case "create":
        try {
          const credential = (await navigator.credentials.create({ publicKey: creationOptions(event.options) })) as PublicKeyCredential | null;
          if (!credential) throw new DOMException("no credential", "NotAllowedError");
          send({ type: "finish", credential: credentialJson(credential) });
        } catch (failure) {
          setError(passkeyError(failure));
          setBusy(false);
        }
        break;
    }
  };

  /** Answers the confirm step with the passkey. */
  const prove = async () => {
    const step = confirm();
    if (!step?.options) return;
    try {
      const credential = (await navigator.credentials.get({ publicKey: requestOptions(step.options) })) as PublicKeyCredential | null;
      if (!credential) throw new DOMException("no credential", "NotAllowedError");
      setConfirm(null);
      send({ type: "proof", credential: credentialJson(credential) });
    } catch (failure) {
      setError(passkeyError(failure));
    }
  };

  /** Answers the confirm step with a recovery code. */
  const proveWithCode = (event: SubmitEvent) => {
    event.preventDefault();
    const code = codeInput?.value.trim() ?? "";
    if (!code) return;
    if (codeInput) codeInput.value = "";
    setConfirm(null);
    send({ type: "proof-code", code });
  };

  /** Stops a change that waits for the confirm step. */
  const cancelConfirm = () => {
    setConfirm(null);
    setAskPassword(false);
    setBusy(false);
  };

  /** Answers the password step of the first passkey. */
  const provePassword = (event: SubmitEvent) => {
    event.preventDefault();
    const password = passwordInput?.value ?? "";
    if (!password) return;
    if (passwordInput) passwordInput.value = "";
    setAskPassword(false);
    send({ type: "proof-password", password });
  };

  const add = (event: SubmitEvent) => {
    event.preventDefault();
    setError(null);
    setCodes(null);
    setBusy(true);
    send({ type: "register", name: name().trim() || "Passkey" });
    setName("");
  };

  const remove = (passkey: PasskeyInfo) => {
    const last = (passkeys()?.length ?? 0) === 1;
    const text = last
      ? `Remove "${passkey.name}"? It is your last passkey: the login then asks only for the password, and the recovery codes stop working.`
      : `Remove "${passkey.name}"? It can no longer log you in.`;
    if (!window.confirm(text)) return;
    setError(null);
    send({ type: "remove", id: passkey.id });
  };

  const newCodes = () => {
    if (!window.confirm("Make new recovery codes? The old codes stop working.")) return;
    setError(null);
    send({ type: "new-codes" });
  };

  return (
    <div class="settings-page settings-passkeys">
      <h2>Passkeys</h2>
      <p>
        With a passkey, each login asks for it after your password: a stolen password alone does not open your session. A passkey can be
        your phone, a security key, or this computer.
      </p>
      <Show when={closed()}>{(text) => <p class="settings-error" role="alert">{text()}</p>}</Show>
      <Show when={!closed()}>
        <Show when={passkeys()} fallback={<p class="settings-note">Loading</p>}>
          {(list) => (
            <>
              <Show when={list().length > 0} fallback={<p class="settings-note">You have no passkey. The login asks only for your password.</p>}>
                <table class="settings-table">
                  <caption>Your passkeys</caption>
                  <thead>
                    <tr>
                      <th scope="col">Name</th>
                      <th scope="col">Host name</th>
                      <th scope="col">Added</th>
                      <th scope="col">Last login</th>
                      <th scope="col">
                        <span class="visually-hidden">Actions</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={list()}>
                      {(passkey) => (
                        <tr>
                          <td>{passkey.name}</td>
                          <td>{passkey.host}</td>
                          <td>{date(passkey.created)}</td>
                          <td>{passkey.used ? date(passkey.used) : "Never"}</td>
                          <td>
                            <button type="button" class="danger" onClick={() => remove(passkey)} aria-label={`Remove ${passkey.name}`}>
                              Remove
                            </button>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
                <p class="settings-note">
                  {codesLeft()} recovery {codesLeft() === 1 ? "code is" : "codes are"} left. A recovery code replaces a lost passkey one time.{" "}
                  <button type="button" onClick={newCodes}>
                    Make new recovery codes
                  </button>
                </p>
              </Show>

              <Show
                when={hasPasskeys()}
                fallback={<p class="settings-error">This browser cannot add a passkey on this page. A passkey needs HTTPS with a valid certificate, or localhost.</p>}
              >
                <form class="settings-form" onSubmit={add}>
                  <label for="passkey-name">Name of the new passkey</label>
                  <input
                    id="passkey-name"
                    type="text"
                    maxlength={64}
                    placeholder="For example: Phone"
                    value={name()}
                    onInput={(event) => setName(event.currentTarget.value)}
                  />
                  <button type="submit" disabled={busy()}>
                    Add a passkey
                  </button>
                </form>
                <p class="settings-note">The passkey belongs to {host()}. It works only when you open farglass with this host name.</p>
              </Show>
            </>
          )}
        </Show>
      </Show>
      <Show when={error()}>{(text) => <p class="settings-error" role="alert">{text()}</p>}</Show>
      <Show when={askPassword()}>
        <section class="settings-confirm" role="dialog" aria-labelledby="password-title">
          <h3 id="password-title">Confirm that it is you</h3>
          <p>Your first passkey needs your password first.</p>
          <form class="settings-form" onSubmit={provePassword}>
            <label for="confirm-password">Your password</label>
            <input
              id="confirm-password"
              type="password"
              autocomplete="current-password"
              ref={(element) => {
                passwordInput = element;
              }}
            />
            <button type="submit">Confirm</button>
          </form>
          <button type="button" onClick={cancelConfirm}>
            Cancel
          </button>
        </section>
      </Show>
      <Show when={confirm()}>
        {(step) => (
          <section class="settings-confirm" role="dialog" aria-labelledby="confirm-title">
            <h3 id="confirm-title">Confirm that it is you</h3>
            <p>A change of your passkeys needs your passkey or a recovery code first.</p>
            <Show when={step().options && hasPasskeys()}>
              <button type="button" class="primary" onClick={() => void prove()}>
                Confirm with my passkey
              </button>
            </Show>
            <Show when={!step().options && step().elsewhere.length > 0}>
              <p class="settings-note">Your passkeys belong to {step().elsewhere.join(" and ")}. Use a recovery code here.</p>
            </Show>
            <form class="settings-form" onSubmit={proveWithCode}>
              <label for="confirm-code">Recovery code</label>
              <input
                id="confirm-code"
                type="text"
                autocomplete="off"
                autocapitalize="off"
                spellcheck={false}
                ref={(element) => {
                  codeInput = element;
                }}
              />
              <button type="submit">Confirm with the code</button>
            </form>
            <button type="button" onClick={cancelConfirm}>
              Cancel
            </button>
          </section>
        )}
      </Show>
      <Show when={codes()}>
        {(list) => (
          <section class="settings-codes" aria-labelledby="codes-title">
            <h3 id="codes-title">Your recovery codes</h3>
            <p>Write them down or save them in your password manager now. This page shows them only one time.</p>
            <ol>
              <For each={list()}>{(code) => <li><code>{code}</code></li>}</For>
            </ol>
            <button type="button" onClick={() => void navigator.clipboard?.writeText(list().join("\n"))}>
              Copy the codes
            </button>{" "}
            <button type="button" onClick={() => setCodes(null)}>
              I saved them
            </button>
          </section>
        )}
      </Show>
    </div>
  );
}
