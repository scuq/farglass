import { type Accessor, createSignal, For } from "solid-js";
import { layoutCode, layoutLabel } from "../keys/layout";
import type { KeyboardSettings } from "../keys/scheme";
import type { DesktopController } from "../wm/controller";
import { Popover } from "./Popover";

/** The keyboard layout of the Linux apps, as the desktop knows it. */
export interface KeyboardState {
  /** The keyboard settings of the user. */
  readonly settings: Accessor<KeyboardSettings>;
  /** The layout that the Linux apps use now, or null before the first keymap. */
  readonly layout: Accessor<string | null>;
  /** Selects a layout, or "auto". */
  readonly select: (layout: "auto" | string) => void;
}

/**
 * The layout of the Linux apps in the panel (F097): a short code, for example
 * DE. A click opens a menu with Automatic, the layouts used last, and a link to
 * Settings > Keyboard.
 */
export function KeyboardIndicator(props: { controller: DesktopController; keyboard: KeyboardState }) {
  const [open, setOpen] = createSignal(false);
  const auto = () => props.keyboard.settings().layout === "auto";
  const current = () => props.keyboard.layout();
  // The name does not have the word "apps": the Apps button has that name.
  const name = () => {
    const layout = current();
    return layout ? `Keyboard layout: ${layoutLabel(layout)}${auto() ? ", automatic" : ""}` : "Keyboard layout";
  };
  const title = () => {
    const layout = current();
    if (!layout) return "Keyboard layout of the Linux apps";
    return `Keyboard layout of the Linux apps: ${layoutLabel(layout)}${auto() ? " (automatic)" : ""}`;
  };
  // The selected layout first, then the other recent layouts.
  const choices = () => {
    const selected = props.keyboard.settings().layout;
    const recent = props.keyboard.settings().recent;
    return selected === "auto" || recent.includes(selected) ? recent : [selected, ...recent];
  };
  const pick = (layout: "auto" | string) => {
    setOpen(false);
    props.keyboard.select(layout);
  };

  return (
    <>
      <button
        type="button"
        class="panel-keyboard"
        aria-haspopup="menu"
        aria-expanded={String(open()) as "true" | "false"}
        aria-controls="keyboard-menu"
        aria-label={name()}
        title={title()}
        onClick={() => setOpen(!open())}
      >
        {current() ? layoutCode(current()!) : "…"}
      </button>
      <Popover id="keyboard-menu" class="panel-keyboard-menu" label="Keyboard layout" open={open()} onClose={() => setOpen(false)}>
        <div role="menu" aria-label="Keyboard layout of the Linux apps">
          <button type="button" role="menuitemradio" aria-checked={String(auto()) as "true" | "false"} onClick={() => pick("auto")}>
            <span>Automatic</span>
            <span class="panel-keyboard-code">{auto() && current() ? layoutCode(current()!) : ""}</span>
          </button>
          <For each={choices()}>
            {(layout) => (
              <button
                type="button"
                role="menuitemradio"
                aria-checked={String(!auto() && props.keyboard.settings().layout === layout) as "true" | "false"}
                onClick={() => pick(layout)}
              >
                <span>{layoutLabel(layout)}</span>
                <span class="panel-keyboard-code">{layoutCode(layout)}</span>
              </button>
            )}
          </For>
          <hr />
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              props.controller.openApp("settings", { page: "keyboard" });
            }}
          >
            More layouts…
          </button>
        </div>
      </Popover>
    </>
  );
}
