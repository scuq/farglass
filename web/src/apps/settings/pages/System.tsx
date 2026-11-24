import { createSignal } from "solid-js";
import type { DbusClient } from "../../../dbus";
import { Loading, type Outcome, OutcomeLine, runAction, Section } from "../common";

const DEST = "org.freedesktop.hostname1";
const PATH = "/org/freedesktop/hostname1";

/** Returns an error text if `name` is not a valid static hostname (RFC 1123), or `null`. */
export function hostnameError(name: string): string | null {
  if (name.length === 0 || name.length > 64) {
    return "A hostname has 1 to 64 characters.";
  }
  if (!/^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$/.test(name)) {
    return "A hostname has only letters, digits, hyphens, and dots, and no hyphen at the start or end of a part.";
  }
  return null;
}

const text = (value: unknown) => (typeof value === "string" && value !== "" ? value : "unknown");

/** The hostname and the facts of the host, from hostname1. */
export function SystemPage(props: { dbus: DbusClient }) {
  const [facts, setFacts] = createSignal<Record<string, unknown> | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [outcome, setOutcome] = createSignal<Outcome>(null);
  let hostInput: HTMLInputElement | undefined;
  let prettyInput: HTMLInputElement | undefined;

  const load = () => props.dbus.getAllProperties(DEST, PATH, DEST).then(setFacts, () => setError("The hostname service of the host is not available."));
  void load();

  const setHostname = (event: SubmitEvent) => {
    event.preventDefault();
    const name = hostInput?.value.trim() ?? "";
    const problem = hostnameError(name);
    if (problem) {
      setOutcome({ kind: "error", text: problem });
      return;
    }
    void runAction(() => props.dbus.call(DEST, PATH, DEST, "SetStaticHostname", "sb", [name, true], { interactive: true }), setOutcome, `The hostname is ${name}.`).then(load);
  };
  const setPretty = (event: SubmitEvent) => {
    event.preventDefault();
    const name = prettyInput?.value.trim() ?? "";
    void runAction(() => props.dbus.call(DEST, PATH, DEST, "SetPrettyHostname", "sb", [name, true], { interactive: true }), setOutcome, "The name of the host changed.").then(load);
  };

  return (
    <div class="settings-page">
      <h2>System</h2>
      <Loading when={facts()} error={error()}>
        {(f) => (
          <>
            <Section id="system-names">
              <div class="settings-fields">
                <form class="settings-row" onSubmit={setHostname}>
                  <label for="settings-hostname">Hostname</label>
                  <input
                    id="settings-hostname"
                    value={String(f().StaticHostname || f().Hostname || "")}
                    autocomplete="off"
                    spellcheck={false}
                    ref={(el) => {
                      hostInput = el;
                    }}
                  />
                  <button type="submit">Set hostname</button>
                </form>
                <form class="settings-row" onSubmit={setPretty}>
                  <label for="settings-pretty">Display name</label>
                  <input
                    id="settings-pretty"
                    value={String(f().PrettyHostname ?? "")}
                    placeholder="For example: Build server"
                    ref={(el) => {
                      prettyInput = el;
                    }}
                  />
                  <button type="submit">Set name</button>
                </form>
              </div>
            </Section>
            <Section id="system-about">
              <dl class="settings-facts">
                <dt>Operating system</dt>
                <dd data-field="os">{text(f().OperatingSystemPrettyName)}</dd>
                <dt>Kernel</dt>
                <dd>{`${text(f().KernelName)} ${typeof f().KernelRelease === "string" ? f().KernelRelease : ""}`.trim()}</dd>
                <dt>Hardware</dt>
                <dd>{[f().HardwareVendor, f().HardwareModel].filter((part) => typeof part === "string" && part !== "").join(" ") || "unknown"}</dd>
                <dt>Chassis</dt>
                <dd>{text(f().Chassis)}</dd>
              </dl>
            </Section>
          </>
        )}
      </Loading>
      <OutcomeLine outcome={outcome()} />
    </div>
  );
}
