/**
 * The server list page of the desktop client (F121).
 *
 * A plain script with no imports: the page has the sandbox and gets only the
 * API of the preload as `window.farglass`.
 */

interface PickerServer {
  id: string;
  name: string;
  url: string;
  pin?: string;
  lastUsed?: number;
}

interface PickerApi {
  list(): Promise<PickerServer[]>;
  add(input: { name: string; url: string }): Promise<PickerServer>;
  update(id: string, input: { name: string; url: string }): Promise<PickerServer>;
  remove(id: string): Promise<void>;
  forgetPin(id: string): Promise<void>;
  connect(id: string): Promise<void>;
  version(): Promise<string>;
  onChanged(listener: (message: string | null) => void): void;
}

{
  const api = (window as unknown as { farglass: PickerApi }).farglass;
  const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
  const list = byId<HTMLUListElement>("servers");
  const empty = byId<HTMLParagraphElement>("empty");
  const message = byId<HTMLParagraphElement>("message");
  const form = byId<HTMLFormElement>("form");
  const formTitle = byId<HTMLHeadingElement>("form-title");
  const name = byId<HTMLInputElement>("name");
  const url = byId<HTMLInputElement>("url");
  const pin = byId<HTMLParagraphElement>("pin");
  const error = byId<HTMLParagraphElement>("error");
  const save = byId<HTMLButtonElement>("save");
  const forget = byId<HTMLButtonElement>("forget");
  const cancel = byId<HTMLButtonElement>("cancel");

  let servers: PickerServer[] = [];
  let editing: string | null = null;

  const show = (el: HTMLElement, text: string | null) => {
    el.textContent = text ?? "";
    el.hidden = !text;
  };

  const run = async (action: () => Promise<unknown>) => {
    show(error, null);
    try {
      await action();
    } catch (e) {
      show(error, e instanceof Error ? e.message : String(e));
    }
  };

  const button = (label: string, className: string, onClick: () => void) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    b.className = className;
    b.addEventListener("click", (event) => {
      event.stopPropagation();
      onClick();
    });
    return b;
  };

  function resetForm(): void {
    editing = null;
    form.reset();
    formTitle.textContent = "Add a server";
    save.textContent = "Add";
    forget.hidden = true;
    cancel.hidden = true;
    show(pin, null);
    show(error, null);
  }

  function edit(server: PickerServer): void {
    editing = server.id;
    name.value = server.name;
    url.value = server.url;
    formTitle.textContent = `Edit ${server.name}`;
    save.textContent = "Save";
    cancel.hidden = false;
    forget.hidden = !server.pin;
    show(pin, server.pin ? `Kept key (SHA-256): ${server.pin}` : "No key kept yet.");
    name.focus();
  }

  function render(): void {
    list.replaceChildren(
      ...servers.map((server) => {
        const item = document.createElement("li");
        item.className = "server";
        item.tabIndex = 0;
        item.setAttribute("aria-label", `Connect to ${server.name}`);
        const text = document.createElement("div");
        const title = document.createElement("div");
        title.className = "name";
        title.textContent = server.name;
        const address = document.createElement("div");
        address.className = "url";
        address.textContent = server.url;
        text.append(title, address);
        const actions = document.createElement("div");
        actions.className = "actions";
        actions.append(
          button("Connect", "", () => void run(() => api.connect(server.id))),
          button("Edit", "secondary", () => edit(server)),
          button("Remove", "danger", () => {
            if (confirm(`Remove ${server.name} from the list? The client forgets its key and its login.`)) {
              void run(async () => {
                await api.remove(server.id);
                if (editing === server.id) resetForm();
                await refresh();
              });
            }
          }),
        );
        item.append(text, actions);
        item.addEventListener("dblclick", () => void run(() => api.connect(server.id)));
        item.addEventListener("keydown", (event) => {
          if (event.key === "Enter" && event.target === item) void run(() => api.connect(server.id));
        });
        return item;
      }),
    );
    empty.hidden = servers.length > 0;
  }

  async function refresh(): Promise<void> {
    servers = await api.list();
    render();
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const input = { name: name.value, url: url.value };
    void run(async () => {
      if (editing) await api.update(editing, input);
      else await api.add(input);
      resetForm();
      await refresh();
    });
  });
  cancel.addEventListener("click", resetForm);
  forget.addEventListener("click", () => {
    const id = editing;
    if (!id) return;
    void run(async () => {
      await api.forgetPin(id);
      await refresh();
      const server = servers.find((s) => s.id === id);
      if (server) edit(server);
    });
  });

  api.onChanged((text) => {
    if (text) show(message, text);
    void refresh();
  });
  void api.version().then((v) => {
    byId<HTMLSpanElement>("version").textContent = `v${v}`;
  });
  void refresh().then(() => {
    const first = list.querySelector<HTMLElement>(".server");
    (first ?? url).focus();
  });
}
