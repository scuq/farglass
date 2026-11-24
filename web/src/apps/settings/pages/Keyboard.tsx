import { createSignal, createUniqueId, For } from "solid-js";
import { ACTION_LABELS, type ActionId, type Binding, chordLabel, DEFAULT_KEYBOARD, detectScheme, KEYBOARD_KEY, type KeyboardSettings, parseKeyboardSettings, SCHEMES, schemeOf } from "../../../keys/scheme";
import { detectLayout, FALLBACK_LAYOUT, LAYOUTS, layoutLabel } from "../../../keys/layout";
import { TOUR_EVENT } from "../../../tour/Tour";
import type { AppContext } from "../../types";

const SCHEME_LABELS = { auto: "Same as this computer", pc: "Windows and Linux", mac: "Mac" } as const;

/** The keybinding scheme, the hints, the list of shortcuts, and the tour (F044), and the layout (F094). */
export function KeyboardPage(props: { context: AppContext }) {
  // Each window of Settings has its own radio groups: two windows must not share a group.
  const group = createUniqueId();
  const [value, setValue] = createSignal<KeyboardSettings>(DEFAULT_KEYBOARD);
  props.context.state.watchShared(KEYBOARD_KEY, (stored) => setValue(parseKeyboardSettings(stored)));
  const change = (patch: Partial<KeyboardSettings>) => {
    const next = { ...value(), ...patch };
    setValue(next);
    props.context.state.setShared(KEYBOARD_KEY, next);
  };
  const scheme = () => schemeOf(value());
  // The layout of the keyboard, or null if this browser cannot tell it (F095).
  // undefined until the browser answers.
  const [detected, setDetected] = createSignal<string | null | undefined>(undefined);
  void detectLayout().then((layout) => setDetected(layout));
  const autoLabel = () => {
    const layout = detected();
    if (layout === undefined) {
      return "Same as this keyboard";
    }
    return layout ? `Same as this keyboard (${layoutLabel(layout)})` : `${layoutLabel(FALLBACK_LAYOUT)}, until you select a layout`;
  };
  const rows = () =>
    (Object.entries(SCHEMES[scheme()]) as [ActionId, readonly Binding[]][]).map(([action, bindings]) => ({
      action,
      keys: bindings.map((binding) => `${chordLabel(binding.chord, scheme())}${binding.lock ? " (Keyboard Lock)" : ""}`),
    }));

  return (
    <div class="settings-page">
      <h2>Keyboard</h2>
      <fieldset>
        <legend>Shortcuts</legend>
        <For each={["auto", "pc", "mac"] as const}>
          {(id) => (
            <label class="settings-choice">
              <input type="radio" name={`${group}-scheme`} value={id} checked={value().scheme === id} onChange={() => change({ scheme: id })} />
              {id === "auto" ? `${SCHEME_LABELS.auto} (${SCHEME_LABELS[detectScheme()]})` : SCHEME_LABELS[id]}
            </label>
          )}
        </For>
      </fieldset>
      <fieldset>
        <legend>Layout of the Linux apps</legend>
        <select name="layout" aria-label="Layout of the Linux apps" onChange={(event) => change({ layout: event.currentTarget.value })}>
          <option value="auto" selected={value().layout === "auto"}>{autoLabel()}</option>
          <For each={LAYOUTS}>{(layout) => <option value={layout.id} selected={value().layout === layout.id}>{layout.label}</option>}</For>
        </select>
      </fieldset>
      <p class="settings-note">The Linux apps type the characters of this layout. Chromium and Edge tell farglass the layout of your keyboard. In a different browser, the layout is English (US) until you select the layout of your keyboard. The terminal and the shell always use the layout of this computer.</p>
      <label class="settings-choice">
        <input type="checkbox" name="hints" checked={value().hints} onChange={(event) => change({ hints: event.currentTarget.checked })} />
        Show the shortcuts when I hold {scheme() === "mac" ? "⌃⇧" : "Alt+Shift"}
      </label>
      <p>
        <button type="button" onClick={() => window.dispatchEvent(new Event(TOUR_EVENT))}>
          Start the tour
        </button>
      </p>
      <table class="settings-keys">
        <thead>
          <tr>
            <th scope="col">Action</th>
            <th scope="col">Keys</th>
          </tr>
        </thead>
        <tbody>
          <For each={rows()}>
            {(row) => (
              <tr data-action={row.action}>
                <td>{ACTION_LABELS[row.action]}</td>
                <td>{row.keys.join(", ") || "–"}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <p class="settings-note">The browser keeps Ctrl+T and Ctrl+W, or Cmd+T and Cmd+W, unless the shell is in fullscreen with Keyboard Lock. Each action has a second key that reaches the shell.</p>
    </div>
  );
}
