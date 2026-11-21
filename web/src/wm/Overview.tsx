import { For } from "solid-js";
import { isShellIcon, windowIcon } from "../launcher/hostApps";
import type { DesktopController } from "./controller";
import { overviewLetters, shownRect } from "./model";

/**
 * The overview (F043): all workspaces side by side in two dimensions, with each window as a
 * box at its place, scaled with a CSS transform. Each window has a letter, and its key gives
 * that window the focus, also on a different workspace. Escape closes.
 */
export function Overview(props: { controller: DesktopController }) {
  const c = props.controller;
  const columns = () => Math.ceil(Math.sqrt(c.desktop().workspaces));
  const rows = () => Math.ceil(c.desktop().workspaces / columns());
  const letters = () => overviewLetters(c.desktop());
  const letterOf = (id: string) => letters().find((entry) => entry.id === id)?.letter ?? null;
  // Each workspace is a cell of the grid, with the proportions of the desktop.
  const scale = () => Math.min(0.9 / columns(), 0.85 / rows());
  const gap = 16;

  const go = (id: string) => {
    c.setOverviewOpen(false);
    c.activate(id);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      c.setOverviewOpen(false);
    } else if (/^[a-z]$/i.test(event.key) && !event.ctrlKey && !event.altKey && !event.metaKey) {
      const entry = letters().find((each) => each.letter === event.key.toLowerCase());
      if (!entry) return;
      go(entry.id);
    } else {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
  };

  return (
    <div class="wm-overview" role="dialog" aria-label="Overview" tabindex="-1" ref={(el) => queueMicrotask(() => el.focus())} onKeyDown={onKeyDown}>
      <p class="wm-overview-hint">Press the letter of a window, or Escape.</p>
      <For each={Array.from({ length: c.desktop().workspaces }, (_, i) => i)}>
        {(workspace) => {
          const w = () => c.area().w * scale();
          const h = () => c.area().h * scale();
          const x = () => (workspace % columns()) * (w() + gap) + (c.area().w - columns() * (w() + gap) + gap) / 2;
          const y = () => Math.floor(workspace / columns()) * (h() + gap) + 48;
          return (
            <section
              class="wm-overview-workspace"
              aria-label={`Workspace ${workspace + 1}`}
              data-current={c.desktop().workspace === workspace ? "true" : undefined}
              style={{ width: `${c.area().w}px`, height: `${c.area().h}px`, transform: `translate(${x()}px, ${y()}px) scale(${scale()})` }}
              onClick={(event) => {
                if (event.target !== event.currentTarget) return;
                c.setOverviewOpen(false);
                c.switchWorkspace(workspace);
              }}
            >
              <span class="wm-overview-number">{workspace + 1}</span>
              <For each={c.desktop().windows.filter((win) => win.workspace === workspace)}>
                {(win) => {
                  const rect = () => shownRect(win, c.area());
                  return (
                    <button
                      type="button"
                      class="wm-overview-window"
                      data-minimized={win.minimized ? "true" : undefined}
                      aria-label={`${win.title}${letterOf(win.id) ? `, key ${letterOf(win.id)!.toUpperCase()}` : ""}`}
                      style={{ width: `${rect().w}px`, height: `${rect().h}px`, transform: `translate(${rect().x}px, ${rect().y}px)` }}
                      onClick={() => go(win.id)}
                    >
                      {windowIcon(win) ? <img src={windowIcon(win)} alt="" data-shell-icon={isShellIcon(windowIcon(win)) ? "true" : undefined} /> : null}
                      <span class="wm-overview-title">{win.title}</span>
                      {letterOf(win.id) ? <kbd class="wm-overview-letter">{letterOf(win.id)!.toUpperCase()}</kbd> : null}
                    </button>
                  );
                }}
              </For>
            </section>
          );
        }}
      </For>
    </div>
  );
}
