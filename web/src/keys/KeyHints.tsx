import { createSignal, For, onCleanup, Show } from "solid-js";
import type { DesktopController } from "../wm/controller";
import { ACTION_LABELS, type ActionId, type Binding, chordLabel, hintModifiers, SCHEMES } from "./scheme";
import "./keys.css";

/** The time that the user holds the modifiers before the hints show. */
export const HINT_DELAY_MS = 700;

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

/**
 * The hints of the shortcuts (F044). Holding the modifiers of the second chords of the scheme,
 * Alt+Shift on a pc and Ctrl+Shift on a Mac, with no other key, shows each action with its key.
 */
export function KeyHints(props: { controller: DesktopController }) {
  const c = props.controller;
  const [shown, setShown] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const held = (event: KeyboardEvent) => {
    const wanted = hintModifiers(c.scheme());
    return event.ctrlKey === wanted.ctrl && event.altKey === wanted.alt && event.shiftKey === wanted.shift && !event.metaKey;
  };
  const stop = () => {
    clearTimeout(timer);
    timer = undefined;
    setShown(false);
  };
  const onDown = (event: KeyboardEvent) => {
    if (!c.keyboard().hints || !MODIFIER_KEYS.has(event.key) || !held(event)) {
      stop();
      return;
    }
    timer ??= setTimeout(() => setShown(true), HINT_DELAY_MS);
  };
  window.addEventListener("keydown", onDown, true);
  window.addEventListener("keyup", stop, true);
  window.addEventListener("blur", stop);
  onCleanup(() => {
    window.removeEventListener("keydown", onDown, true);
    window.removeEventListener("keyup", stop, true);
    window.removeEventListener("blur", stop);
    clearTimeout(timer);
  });

  const rows = () => {
    const wanted = hintModifiers(c.scheme());
    return (Object.entries(SCHEMES[c.scheme()]) as [ActionId, readonly Binding[]][]).flatMap(([action, bindings]) =>
      bindings
        .filter((each) => !each.lock && !!each.chord.ctrl === wanted.ctrl && !!each.chord.alt === wanted.alt && !!each.chord.shift === wanted.shift && !each.chord.meta)
        .map((each) => ({ action, key: chordLabel(each.chord, c.scheme()) })),
    );
  };

  return (
    <Show when={shown()}>
      <aside class="key-hints" aria-label="Shortcuts">
        <ul>
          <For each={rows()}>
            {(row) => (
              <li>
                <kbd>{row.key}</kbd>
                <span>{ACTION_LABELS[row.action]}</span>
              </li>
            )}
          </For>
        </ul>
      </aside>
    </Show>
  );
}
