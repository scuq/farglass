import { createSignal, onCleanup, Show } from "solid-js";
import { ACTION_LABELS, type ActionId } from "../keys/scheme";
import type { DesktopController } from "../wm/controller";
import "../keys/keys.css";

/** The steps of the tour: each one teaches the key of one action. */
export const TOUR_STEPS: readonly ActionId[] = [
  "launcher",
  "new-terminal",
  "switch",
  "maximize",
  "snap-left",
  "layouts",
  "overview",
  "workspace-right",
  "workspace-left",
  "close-window",
];

/** The event that starts the tour, for example from the settings. */
export const TOUR_EVENT = "farglass-tour";

/**
 * The interactive tour of the shortcuts (F044). Each step shows an action and its key in the
 * scheme, and continues when the user presses that key. Skip goes to the next step.
 */
export function Tour(props: { controller: DesktopController }) {
  const c = props.controller;
  const [step, setStep] = createSignal(0);
  const [done, setDone] = createSignal(false);
  const action = () => TOUR_STEPS[step()];
  const next = () => {
    setDone(false);
    if (step() + 1 >= TOUR_STEPS.length) {
      c.setTourOpen(false);
      return;
    }
    setStep(step() + 1);
  };
  onCleanup(
    c.onAction((pressed) => {
      if (pressed !== action() || done()) return;
      setDone(true);
      setTimeout(next, 900);
    }),
  );
  return (
    <aside class="tour" role="dialog" aria-label="Keyboard tour">
      <p class="tour-step">
        Step {step() + 1} of {TOUR_STEPS.length}
      </p>
      <h2>{ACTION_LABELS[action()!]}</h2>
      <p class="tour-keys">
        Press <kbd>{c.hint(action()!) ?? "–"}</kbd>
      </p>
      <Show when={done()}>
        <p class="tour-done" role="status">
          Done.
        </p>
      </Show>
      <div class="tour-buttons">
        <button type="button" onClick={next}>
          Skip
        </button>
        <button type="button" onClick={() => c.setTourOpen(false)}>
          End the tour
        </button>
      </div>
    </aside>
  );
}
