import { For, Show } from "solid-js";
import type { TrayItem } from "../proto";
import { iconUrl, itemTip, scrollDelta, tray } from "./tray";

/**
 * The system tray in the panel (F102): one button for each item of a program
 * of the host. A click does the main action of the item: most programs show
 * their window. The middle button does the second action.
 */
export function Tray() {
  const items = () => tray()?.items() ?? [];
  return (
    <Show when={items().length > 0}>
      <nav class="panel-tray" aria-label="System tray">
        <For each={items()} keyed={(item: TrayItem) => item.id}>
          {(item) => (
            <button
              type="button"
              class="panel-tray-item"
              aria-label={item().title}
              title={itemTip(item())}
              data-status={item().status}
              onClick={(event) => tray()?.activate(item().id, event.clientX, event.clientY)}
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
          )}
        </For>
      </nav>
    </Show>
  );
}
