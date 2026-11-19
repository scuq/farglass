import { createSignal, Show } from "solid-js";
import type { DbusClient } from "../../../dbus";
import { ConfirmDialog, createConfirm, type Outcome, OutcomeLine, runAction } from "../common";

const LOGIN1 = "org.freedesktop.login1";
const LOGIN1_PATH = "/org/freedesktop/login1";
const MANAGER = "org.freedesktop.login1.Manager";

type Action = "Reboot" | "PowerOff";

const ACTIONS: Record<Action, { label: string; title: string; text: string; check: string }> = {
  Reboot: {
    label: "Restart",
    title: "Restart the host?",
    text: "Each session on the host ends, also this one, and work that is not saved is lost. You can connect again after the host started.",
    check: "CanReboot",
  },
  PowerOff: {
    label: "Power off",
    title: "Power off the host?",
    text: "Each session on the host ends, also this one, and work that is not saved is lost. You cannot connect again until a person or a remote management board starts the host.",
    check: "CanPowerOff",
  },
};

/** Restart and power off through login1. Each action needs a confirm step, and polkit can ask for a password. */
export function PowerPage(props: { dbus: DbusClient }) {
  const [can, setCan] = createSignal<Record<Action, string>>({ Reboot: "unknown", PowerOff: "unknown" });
  const [outcome, setOutcome] = createSignal<Outcome>(null);
  const confirm = createConfirm<Action>();
  for (const action of Object.keys(ACTIONS) as Action[]) {
    props.dbus.call(LOGIN1, LOGIN1_PATH, MANAGER, ACTIONS[action].check).then(
      ([answer]) => setCan((old) => ({ ...old, [action]: String(answer) })),
      () => setCan((old) => ({ ...old, [action]: "na" })),
    );
  }

  const run = (action: Action) => {
    confirm.clear();
    void runAction(() => props.dbus.call(LOGIN1, LOGIN1_PATH, MANAGER, action, "b", [true], { interactive: true }), setOutcome, "The host stops now.");
  };

  const available = (action: Action) => can()[action] === "yes" || can()[action] === "challenge";

  return (
    <div class="settings-page">
      <h2>Power</h2>
      <p>These actions stop each session on the host, also this one.</p>
      <div class="settings-buttons settings-buttons-start">
        {(Object.keys(ACTIONS) as Action[]).map((action) => (
          <button type="button" class="danger" disabled={!available(action)} onClick={() => confirm.ask(action)}>
            {ACTIONS[action].label}
          </button>
        ))}
      </div>
      <Show when={can().Reboot === "challenge" || can().PowerOff === "challenge"}>
        <p class="settings-note">The host asks for an administrator password.</p>
      </Show>
      <OutcomeLine outcome={outcome()} />
      <Show when={confirm.pending()}>
        {(action) => (
          <ConfirmDialog title={ACTIONS[action()].title} action={ACTIONS[action()].label} onconfirm={() => run(action())} oncancel={confirm.clear}>
            <p>{ACTIONS[action()].text}</p>
          </ConfirmDialog>
        )}
      </Show>
    </div>
  );
}
