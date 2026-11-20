import { createSignal, For, onCleanup, Show } from "solid-js";
import { creationOptions, credentialJson, hasPasskeys, passkeyError } from "../../../passkey";
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
