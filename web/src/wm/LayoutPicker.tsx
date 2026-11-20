import { createSignal, For } from "solid-js";
import { windowIcon } from "../launcher/hostApps";
import type { DesktopController } from "./controller";
import { LAYOUT_LABELS, LAYOUTS, type Layout, layoutRects } from "./geometry";
import { switchable, windowById } from "./model";

/** The size of a preview in pixels. */
const PREVIEW = { w: 176, h: 110 };

/**
 * The layout picker (F043): each layout as a preview with the open windows of the workspace.
 * The arrow keys or 1 to 5 select a layout, and Enter or a click applies it. Escape closes.
 */
export function LayoutPicker(props: { controller: DesktopController }) {
  const c = props.controller;
  const [active, setActive] = createSignal(0);
  const ids = () => switchable(c.desktop()).filter((id) => !windowById(c.desktop(), id)?.minimized);
  const scale = () => Math.min(PREVIEW.w / c.area().w, PREVIEW.h / c.area().h);

  const onKeyDown = (event: KeyboardEvent) => {
    const count = LAYOUTS.length;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") setActive((active() + 1) % count);
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") setActive((active() + count - 1) % count);
    else if (event.key === "Enter") c.applyLayout(LAYOUTS[active()]!);
    else if (event.key === "Escape") c.setLayoutsOpen(false);
    else if (/^[1-5]$/.test(event.key)) c.applyLayout(LAYOUTS[Number(event.key) - 1]!);
    else return;
    event.preventDefault();
    event.stopPropagation();
  };

  return (
    <div class="wm-layouts-backdrop" onPointerDown={(event) => event.target === event.currentTarget && c.setLayoutsOpen(false)}>
      <div class="wm-layouts" role="dialog" aria-label="Layouts" tabindex="-1" ref={(el) => queueMicrotask(() => el.focus())} onKeyDown={onKeyDown}>
        <p class="wm-layouts-hint">Select a layout for the windows of this workspace: arrow keys, or 1 to 5, and Enter.</p>
        <div class="wm-layouts-list" role="listbox" aria-label="Layouts">
          <For each={LAYOUTS}>
            {(layout: Layout, index) => (
              <button
                type="button"
                role="option"
                class="wm-layout"
                data-layout={layout}
                aria-selected={String(index() === active()) as "true" | "false"}
                onPointerEnter={() => setActive(index())}
                onClick={() => c.applyLayout(layout)}
              >
                <span class="wm-layout-preview" style={{ width: `${PREVIEW.w}px`, height: `${PREVIEW.h}px` }}>
                  <For each={layoutRects(layout, Math.max(ids().length, 1), c.area())}>
                    {(rect, place) => {
                      const id = () => ids()[place()];
                      const win = () => (id() ? windowById(c.desktop(), id()!) : undefined);
                      return (
                        <span
                          class="wm-layout-window"
                          data-first={place() === 0 ? "true" : undefined}
                          style={{
                            width: `${rect.w * scale()}px`,
                            height: `${rect.h * scale()}px`,
                            transform: `translate(${rect.x * scale()}px, ${rect.y * scale()}px)`,
                          }}
                        >
                          {win() && windowIcon(win()!) ? <img src={windowIcon(win()!)} alt="" /> : null}
                          <span class="wm-layout-title">{win()?.title ?? ""}</span>
                        </span>
                      );
                    }}
                  </For>
                </span>
                <span class="wm-layout-name">
                  {index() + 1} {LAYOUT_LABELS[layout]}
                </span>
              </button>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
