import { createSignal, createUniqueId, For, Show } from "solid-js";
import { DEFAULT_TERMINAL, FONT_SIZES, FONTS, fontFamily, PROFILE_IDS, paletteOf, parseTerminal, profileLabel, SCROLLBACKS, TERMINAL_KEY, type TerminalSettings } from "../../terminal/settings";
import type { AppContext } from "../../types";
import { Section } from "../common";

/** The settings of the terminal app (F107). The value is in the state store, thus each terminal gets a change at once. */
export function TerminalPage(props: { context: AppContext }) {
  const group = createUniqueId();
  const [value, setValue] = createSignal<TerminalSettings>(DEFAULT_TERMINAL);
  props.context.state.watchShared(TERMINAL_KEY, (stored) => setValue(parseTerminal(stored)));
  const change = (patch: Partial<TerminalSettings>) => {
    const next = { ...value(), ...patch };
    setValue(next);
    props.context.state.setShared(TERMINAL_KEY, next);
  };

  return (
    <div class="settings-page">
      <h2>Terminal</h2>
      <p class="settings-note">A change applies at once to each open terminal in each tab of your session.</p>
      <Section id="terminal-colors" group>
        <div class="settings-choices">
          <For each={PROFILE_IDS}>
            {(id) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-colors`} value={id} checked={value().colors === id} onChange={() => change({ colors: id })} />
                <span class="term-profile">
                  {profileLabel(id)}
                  <Show when={paletteOf(id)}>
                    {(palette) => (
                      <span class="term-profile-sample" aria-hidden="true" style={{ background: palette().background, color: palette().foreground }}>
                        <For each={palette().ansi.slice(0, 8)}>{(color) => <span class="term-profile-cell" style={{ background: color }} />}</For>
                      </span>
                    )}
                  </Show>
                </span>
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="terminal-font">
        <div class="settings-fields">
          <div class="settings-row">
            <label for={`${group}-font`}>Font</label>
            <select id={`${group}-font`} class="settings-select" onChange={(event) => change({ font: event.currentTarget.value as TerminalSettings["font"] })}>
              <For each={FONTS}>
                {(font) => (
                  <option value={font} selected={value().font === font}>
                    {font === "default" ? "Default monospace font" : font}
                  </option>
                )}
              </For>
            </select>
            <span />
          </div>
          <div class="settings-row">
            <label for={`${group}-size`}>Size</label>
            <select id={`${group}-size`} class="settings-select" onChange={(event) => change({ fontSize: Number(event.currentTarget.value) as TerminalSettings["fontSize"] })}>
              <For each={FONT_SIZES}>
                {(size) => (
                  <option value={size} selected={value().fontSize === size}>
                    {size} px
                  </option>
                )}
              </For>
            </select>
            <span />
          </div>
        </div>
        <p class="term-font-sample" style={{ "font-family": fontFamily(value().font), "font-size": `${value().fontSize}px` }}>
          $ ls -l /etc 0O 1lI {"{}"} =&gt; !=
        </p>
        <p class="settings-note">The font must be installed on this computer. If it is not, the terminal uses the default monospace font.</p>
      </Section>
      <Section id="terminal-scrollback" group>
        <div class="settings-choices settings-choices-row">
          <For each={SCROLLBACKS}>
            {(lines) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-scrollback`} value={lines} checked={value().scrollback === lines} onChange={() => change({ scrollback: lines })} />
                {lines.toLocaleString("en-US")} lines
              </label>
            )}
          </For>
        </div>
        <p class="settings-note">The number of lines that you can scroll back in a terminal. More lines use more memory in this browser.</p>
      </Section>
    </div>
  );
}
