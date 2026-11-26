/**
 * The tab strip of the desktop client (F122).
 *
 * A plain script with no imports: the page has the sandbox and gets only the
 * API of its preload as `window.farglassTabs`.
 */

interface TabsState {
  active: string;
  tabs: { id: string; name: string; title: string }[];
}

interface TabsApi {
  select(id: string): Promise<unknown>;
  close(id: string): Promise<unknown>;
  onState(listener: (state: TabsState) => void): void;
}

{
  const api = (window as unknown as { farglassTabs: TabsApi }).farglassTabs;
  const strip = document.getElementById("strip") as HTMLElement;

  const tab = (id: string, label: string, title: string, selected: boolean, closable: boolean) => {
    const el = document.createElement("div");
    el.className = closable ? "tab" : "tab servers";
    el.setAttribute("role", "tab");
    el.setAttribute("aria-selected", String(selected));
    el.tabIndex = 0;
    el.title = title;
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label;
    el.append(text);
    el.addEventListener("click", () => void api.select(id));
    el.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") void api.select(id);
    });
    if (closable) {
      const close = document.createElement("span");
      close.className = "close";
      close.textContent = "×";
      close.title = `Close ${label}`;
      close.setAttribute("role", "button");
      close.setAttribute("aria-label", `Close ${label}`);
      close.addEventListener("click", (event) => {
        event.stopPropagation();
        void api.close(id);
      });
      el.append(close);
      // A click with the middle button closes the tab, as in a browser.
      el.addEventListener("auxclick", (event) => {
        if (event.button === 1) void api.close(id);
      });
    }
    return el;
  };

  api.onState((state) => {
    const fill = document.createElement("div");
    fill.className = "fill";
    strip.replaceChildren(
      tab("servers", "Servers", "Server list (Ctrl+Shift+F12)", state.active === "servers", false),
      ...state.tabs.map((t) => tab(t.id, t.name, t.title ? `${t.name}: ${t.title}` : t.name, state.active === t.id, true)),
      fill,
    );
  });
}
