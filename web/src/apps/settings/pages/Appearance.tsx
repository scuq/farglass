import { createSignal, createUniqueId, For } from "solid-js";
import { ACCENTS, APPEARANCE_KEY, type Appearance, DEFAULT_APPEARANCE, MOTIONS, parseAppearance, TEXT_SIZES, THEMES } from "../../../appearance";
import { parseVideoMode, VIDEO_KEY, type VideoMode } from "../../../surface/redirect";
import type { AppContext } from "../../types";
import { Section } from "../common";

const THEME_LABELS: Record<Appearance["theme"], string> = { lcars: "LCARS", system: "Same as this computer", light: "Light", dark: "Dark" };
const MOTION_LABELS: Record<Appearance["motion"], string> = { system: "Same as this computer", full: "Full", reduced: "Reduced: no movement, no pulse, and no blinking cursor" };
const ACCENT_LABELS: Record<Appearance["accent"], string> = { blue: "Blue", green: "Green", purple: "Purple", orange: "Orange" };

/** The layouts of the panel (F098). */
const PANEL_CHOICES: readonly { id: Appearance["panel"]; label: string }[] = [
  { id: "bar", label: "Top bar" },
  { id: "dock", label: "Dock on the left" },
];

/** The appearance of the shell. The value is in the state store, thus each tab and each login gets it. */
export function AppearancePage(props: { context: AppContext }) {
  // Each window of Settings has its own radio groups: two windows must not share a group.
  const group = createUniqueId();
  const [value, setValue] = createSignal<Appearance>(DEFAULT_APPEARANCE);
  const [video, setVideo] = createSignal<VideoMode>("smart");
  props.context.state.watchShared(VIDEO_KEY, (stored) => setVideo(parseVideoMode(stored)));
  const changeVideo = (mode: VideoMode) => {
    setVideo(mode);
    props.context.state.setShared(VIDEO_KEY, { mode });
  };
  props.context.state.watchShared(APPEARANCE_KEY, (stored) => setValue(parseAppearance(stored)));
  const change = (patch: Partial<Appearance>) => {
    const next = { ...value(), ...patch };
    setValue(next);
    props.context.state.setShared(APPEARANCE_KEY, next);
  };

  return (
    <div class="settings-page">
      <h2>Appearance</h2>
      <p class="settings-note">The appearance applies at once in each tab of your session.</p>
      <Section id="appearance-theme" group>
        <div class="settings-choices settings-choices-row">
          <For each={THEMES}>
            {(theme) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-theme`} value={theme} checked={value().theme === theme} onChange={() => change({ theme })} />
                {THEME_LABELS[theme]}
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="appearance-accent" group>
        <div class="settings-choices settings-choices-row">
          <For each={ACCENTS}>
            {(accent) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-accent`} value={accent} checked={value().accent === accent} onChange={() => change({ accent })} />
                <span class="settings-swatch" data-accent={accent} aria-hidden="true" />
                {ACCENT_LABELS[accent]}
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="appearance-text" group>
        <div class="settings-choices settings-choices-row">
          <For each={TEXT_SIZES}>
            {(size) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-text-size`} value={size} checked={value().textSize === size} onChange={() => change({ textSize: size })} />
                {size} %
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="appearance-motion" group>
        <div class="settings-choices">
          <For each={MOTIONS}>
            {(motion) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-motion`} value={motion} checked={value().motion === motion} onChange={() => change({ motion })} />
                {MOTION_LABELS[motion]}
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="appearance-panel" group>
        <div class="settings-choices settings-choices-row">
          <For each={PANEL_CHOICES}>
            {(choice) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-panel`} value={choice.id} checked={value().panel === choice.id} onChange={() => change({ panel: choice.id })} />
                {choice.label}
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="appearance-video" group>
        <div class="settings-choices">
          <label class="settings-choice">
            <input type="radio" name={`${group}-video`} value="client" checked={video() === "client"} onChange={() => changeVideo("client")} />
            Play in this browser: sharp, and with little load on the host. The controls of the page can show small marks over the video.
          </label>
          <label class="settings-choice">
            <input type="radio" name={`${group}-video`} value="host" checked={video() === "host"} onChange={() => changeVideo("host")} />
            Play on the host: the window shows the video as the host draws it, with more load on the host and the connection.
          </label>
          <label class="settings-choice">
            <input type="radio" name={`${group}-video`} value="smart" checked={video() === "smart"} onChange={() => changeVideo("smart")} />
            Smart (the default): play in this browser, and on the host while the pointer is on the video. The controls of the page show with no marks.
          </label>
        </div>
      </Section>
    </div>
  );
}
