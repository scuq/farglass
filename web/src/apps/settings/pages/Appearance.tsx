import { createSignal, For } from "solid-js";
import { ACCENTS, APPEARANCE_KEY, type Appearance, DEFAULT_APPEARANCE, parseAppearance, TEXT_SIZES, THEMES } from "../../../appearance";
import type { AppContext } from "../../types";

const THEME_LABELS: Record<Appearance["theme"], string> = { system: "Same as this computer", light: "Light", dark: "Dark" };
const ACCENT_LABELS: Record<Appearance["accent"], string> = { blue: "Blue", green: "Green", purple: "Purple", orange: "Orange" };

/** The appearance of the shell. The value is in the state store, thus each tab and each login gets it. */
export function AppearancePage(props: { context: AppContext }) {
  const [value, setValue] = createSignal<Appearance>(DEFAULT_APPEARANCE);
  props.context.state.watchShared(APPEARANCE_KEY, (stored) => setValue(parseAppearance(stored)));
  const change = (patch: Partial<Appearance>) => {
    const next = { ...value(), ...patch };
    setValue(next);
    props.context.state.setShared(APPEARANCE_KEY, next);
  };

  return (
    <div class="settings-page">
      <h2>Appearance</h2>
      <fieldset>
        <legend>Theme</legend>
        <For each={THEMES}>
          {(theme) => (
            <label class="settings-choice">
              <input type="radio" name="theme" value={theme} checked={value().theme === theme} onChange={() => change({ theme })} />
              {THEME_LABELS[theme]}
            </label>
          )}
        </For>
      </fieldset>
      <fieldset>
        <legend>Accent color</legend>
        <For each={ACCENTS}>
          {(accent) => (
            <label class="settings-choice">
              <input type="radio" name="accent" value={accent} checked={value().accent === accent} onChange={() => change({ accent })} />
              <span class="settings-swatch" data-accent={accent} aria-hidden="true" />
              {ACCENT_LABELS[accent]}
            </label>
          )}
        </For>
      </fieldset>
      <fieldset>
        <legend>Text size</legend>
        <For each={TEXT_SIZES}>
          {(size) => (
            <label class="settings-choice">
              <input type="radio" name="text-size" value={size} checked={value().textSize === size} onChange={() => change({ textSize: size })} />
              {size} %
            </label>
          )}
        </For>
      </fieldset>
      <p class="settings-note">The appearance applies at once in each tab of your session.</p>
    </div>
  );
}
