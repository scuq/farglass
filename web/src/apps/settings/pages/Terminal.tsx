import { createSignal, createUniqueId, For, Show } from "solid-js";
import { DEFAULT_TERMINAL, FONT_SIZES, FONTS, fontFamily, NEW_TABS, ON_EXITS, PROFILE_IDS, paletteOf, parseTerminal, profileLabel, SCROLLBACKS, TAB_BARS, TERMINAL_KEY, type TerminalSettings } from "../../terminal/settings";
import type { AppContext } from "../../types";
import { Section } from "../common";

const TAB_BAR_LABELS: Record<TerminalSettings["tabBar"], string> = { auto: "Show the tab bar when the window has two or more tabs", always: "Always show the tab bar" };
const ON_EXIT_LABELS: Record<TerminalSettings["onExit"], string> = { close: "Close the tab when its shell stops. The last tab stays open.", keep: "Keep the tab open when its shell stops. Press Enter to start a new shell." };
const NEW_TAB_LABELS: Record<TerminalSettings["newTab"], string> = { end: "Open a new tab at the end", next: "Open a new tab after the active tab" };

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
      <Section id="terminal-tabs" group>
        <p class="settings-note">To open a tab, select + in the tab bar, or press Alt+Shift+Enter. To go to the next or the previous tab, press Alt+PageDown or Alt+PageUp.</p>
        <div class="settings-choices">
          <For each={TAB_BARS}>
            {(tabBar) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-tab-bar`} value={tabBar} checked={value().tabBar === tabBar} onChange={() => change({ tabBar })} />
                {TAB_BAR_LABELS[tabBar]}
              </label>
            )}
          </For>
        </div>
        <div class="settings-choices">
          <For each={NEW_TABS}>
            {(newTab) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-new-tab`} value={newTab} checked={value().newTab === newTab} onChange={() => change({ newTab })} />
                {NEW_TAB_LABELS[newTab]}
              </label>
            )}
          </For>
        </div>
        <div class="settings-choices">
          <For each={ON_EXITS}>
            {(onExit) => (
              <label class="settings-choice">
                <input type="radio" name={`${group}-on-exit`} value={onExit} checked={value().onExit === onExit} onChange={() => change({ onExit })} />
                {ON_EXIT_LABELS[onExit]}
              </label>
            )}
          </For>
        </div>
      </Section>
      <Section id="terminal-splits" group>
        <p class="settings-note">To split a pane, press Alt+Shift+\ for a pane on the right, or Alt+Shift+- for a pane below. You can also right-click the terminal. To go to the next pane, press Alt+Shift with an arrow key. Drag the border between two panes to change their size.</p>
        <div class="settings-choices">
          <label class="settings-choice">
            <input type="checkbox" checked={value().dimInactive} onChange={(event) => change({ dimInactive: event.currentTarget.checked })} />
            Dim the panes that do not have the focus
          </label>
          <label class="settings-choice">
            <input type="checkbox" checked={value().focusFollowsMouse} onChange={(event) => change({ focusFollowsMouse: event.currentTarget.checked })} />
            Give the focus to the pane under the pointer
          </label>
        </div>
      </Section>
    </div>
  );
}
