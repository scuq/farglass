import { For, onCleanup, Show } from "solid-js";
import { setContextMenu } from "../menu/menu";
import type { TrayItem } from "../proto";
import { iconUrl, itemTip, scrollDelta, tray } from "./tray";

/**
 * The system tray in the panel (F102): one button for each item of a program
 * of the host. A click does the main action of the item: most programs show
 * their window. The middle button does the second action. A right click,
 * Shift+F10, or the ContextMenu key opens the menu of the program (F103).
 */
export function Tray() {
  const items = () => tray()?.items() ?? [];
  return (
    <Show when={items().length > 0}>
      <nav class="panel-tray" aria-label="System tray">
        <For each={items()} keyed={(item: TrayItem) => item.id}>
          {(item) => {
            let removeMenu: (() => void) | undefined;
            onCleanup(() => removeMenu?.());
            return (
              <button
                type="button"
                class="panel-tray-item"
                aria-label={item().title}
                aria-haspopup={item().menu ? "menu" : undefined}
                title={itemTip(item())}
                data-status={item().status}
                ref={(el) => {
                  // The menu comes from the agent after the click. The provider asks for it and gives no items now.
                  removeMenu = setContextMenu(el, () => {
                    tray()?.openMenu(item(), el);
                    return [];
                  });
                }}
                onClick={(event) => tray()?.click(item(), event.currentTarget, event.clientX, event.clientY)}
                onAuxClick={(event) => {
                  if (event.button === 1) tray()?.secondaryActivate(item().id, event.clientX, event.clientY);
                }}
                onWheel={(event) => {
                  const horizontal = Math.abs(event.deltaX) > Math.abs(event.deltaY);
                  tray()?.scroll(item().id, scrollDelta(horizontal ? event.deltaX : event.deltaY), horizontal);
                }}
              >
                {iconUrl(item()) ? <img class="panel-tray-icon" src={iconUrl(item())} alt="" /> : <span class="panel-tray-letter">{item().title.slice(0, 1).toUpperCase()}</span>}
              </button>
            );
          }}
        </For>
      </nav>
    </Show>
  );
}
