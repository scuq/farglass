/**
 * The main process of the desktop client of farglass (F121, F122, ADR-060).
 *
 * The client has one window with tabs. The first tab is the list of servers.
 * Each other tab is the shell of one server, loaded from farglassd. A tab of
 * the shell has the sandbox, no preload, and its own session for each server.
 * A self-signed certificate is accepted only if the SHA-256 of its public key
 * is the pin of the server. The user accepts the pin at the first connection.
 *
 * A tray icon shows and hides the window. A close of the window hides it in the
 * tray, if the user did not turn that off.
 */
import { BrowserWindow, Menu, Tray, WebContentsView, app, dialog, ipcMain, nativeImage, session, shell } from "electron";
import type { IpcMainInvokeEvent, MenuItemConstructorOptions, Session, WebContents } from "electron";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { HOST_PIN_COMMAND, checkPin, publicKeyPin } from "./pin";
import { ServerStore } from "./servers";
import type { Server, ServerInput } from "./servers";

/** The permissions that the shell can get, only from the origin of its server. */
const ALLOWED_PERMISSIONS = new Set([
  "media",
  "clipboard-read",
  "clipboard-sanitized-write",
  "fullscreen",
  "keyboardLock",
  "pointerLock",
  "notifications",
]);

/** The background of the window while a page loads: the dark background of the shell. */
const BACKGROUND = "#000000";

/** The height of the tab strip in pixels. */
const STRIP_HEIGHT = 36;

/** The id of the tab with the server list. */
const SERVERS = "servers";

/** One tab with the shell of a server. */
interface Tab {
  server: Server;
  view: WebContentsView;
  title: string;
  /** Why the client refused the certificate of the server, if it did. */
  refusal: Refusal | null;
}

/** Why the client refused the certificate of a server: the user said no, or the key changed. */
type Refusal = { kind: "declined" } | { kind: "changed"; kept: string; now: string };

let store: ServerStore;
let win: BrowserWindow | null = null;
let pickerView: WebContentsView | null = null;
let tray: Tray | null = null;
let quitting = false;
/** The tabs of the shell by server id, in the order of the strip. */
const tabs = new Map<string, Tab>();
let active = SERVERS;

const staticFile = (name: string) => join(__dirname, "..", "static", name);
const fileUrl = (name: string) => pathToFileURL(staticFile(name)).href;

const originOf = (url: string): string | null => {
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
};

/** Opens a link of another origin in the browser of the desktop, if it is http or https. */
function openOutside(url: string): void {
  const protocol = originOf(url) && new URL(url).protocol;
  if (protocol === "https:" || protocol === "http:") void shell.openExternal(url);
}

// The window and the tabs

/** Puts each view at its place. In full screen, the active shell fills the window and the strip is hidden. */
function layout(): void {
  if (!win || win.isDestroyed()) return;
  const { width, height } = win.getContentBounds();
  const full = win.isFullScreen() && active !== SERVERS;
  const top = full ? 0 : STRIP_HEIGHT;
  const place = (view: WebContentsView, visible: boolean) => {
    view.setVisible(visible);
    if (visible) view.setBounds({ x: 0, y: top, width, height: Math.max(0, height - top) });
  };
  if (pickerView) place(pickerView, active === SERVERS);
  for (const [id, tab] of tabs) place(tab.view, active === id);
}

/** Sends the tabs to the strip, sets the title of the window, and changes the tray menu. */
function publish(): void {
  if (!win || win.isDestroyed()) return;
  const list = [...tabs.entries()].map(([id, tab]) => ({ id, name: tab.server.name, title: tab.title }));
  win.webContents.send("tabs:state", { active, tabs: list });
  const current = tabs.get(active);
  win.setTitle(current ? `${current.server.name} — farglass` : "farglass");
  updateTray();
}

const alive = (): boolean => win !== null && !win.isDestroyed();

function activate(id: string): void {
  if (!alive()) return;
  if (id !== SERVERS && !tabs.has(id)) id = SERVERS;
  active = id;
  if (win?.isFullScreen() && id === SERVERS) win.setFullScreen(false);
  layout();
  publish();
  const view = id === SERVERS ? pickerView : tabs.get(id)?.view;
  view?.webContents.focus();
}

/** Selects the tab `step` places after the active tab, round the end. The server list is the first tab. */
function cycle(step: number): void {
  const ids = [SERVERS, ...tabs.keys()];
  const index = ids.indexOf(active);
  activate(ids[(index + step + ids.length) % ids.length] ?? SERVERS);
}

function showWindow(): void {
  if (!win || win.isDestroyed()) createWindow();
  win!.show();
  win!.focus();
}

function toggleWindow(): void {
  if (win && !win.isDestroyed() && win.isVisible() && win.isFocused()) win.hide();
  else showWindow();
}

/** Shows the server list, with a message if `message` is set. */
function showServers(message?: string): void {
  showWindow();
  activate(SERVERS);
  pickerView?.webContents.send("servers:changed", message ?? null);
}

/** Tells the server list that the list changed. */
function notifyPicker(): void {
  pickerView?.webContents.send("servers:changed", null);
}

/** Ctrl+Shift+F12 shows the server list. Ctrl+Shift+F10 and Ctrl+Shift+F11 select the tab before and after. */
function addShortcuts(contents: WebContents): void {
  contents.on("before-input-event", (event, input) => {
    if (input.type !== "keyDown" || !input.control || !input.shift || input.alt || input.meta) return;
    const action = { F10: () => cycle(-1), F11: () => cycle(1), F12: () => activate(SERVERS) }[input.code];
    if (action) {
      event.preventDefault();
      action();
    }
  });
}

function createWindow(): void {
  win = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 480,
    minHeight: 360,
    title: "farglass",
    icon: staticFile("tray.png"),
    backgroundColor: BACKGROUND,
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, "tabsPreload.js"),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  const w = win;
  void w.loadFile(staticFile("tabs.html"));
  w.webContents.on("did-finish-load", publish);
  addShortcuts(w.webContents);

  pickerView = new WebContentsView({
    webPreferences: { preload: join(__dirname, "preload.js"), sandbox: true, contextIsolation: true, nodeIntegration: false },
  });
  pickerView.setBackgroundColor(BACKGROUND);
  w.contentView.addChildView(pickerView);
  void pickerView.webContents.loadFile(staticFile("index.html"));
  addShortcuts(pickerView.webContents);

  for (const event of ["resize", "enter-full-screen", "leave-full-screen", "enter-html-full-screen", "leave-html-full-screen"] as const) {
    w.on(event as "resize", () => setImmediate(layout));
  }
  w.on("close", (event) => {
    if (quitting) return;
    if (store.closeToTray && tray) {
      event.preventDefault();
      w.hide();
      return;
    }
    if (tabs.size > 0) {
      event.preventDefault();
      void dialog
        .showMessageBox(w, {
          type: "question",
          buttons: ["Quit farglass", "Stay"],
          defaultId: 1,
          cancelId: 1,
          title: "Quit farglass?",
          message: `Quit farglass and close ${tabs.size === 1 ? "one session" : `${tabs.size} sessions`}?`,
          detail: SESSION_CONTINUES,
        })
        .then(({ response }) => {
          if (response === 0) quit();
        });
      return;
    }
    quitting = true;
  });
  w.on("closed", () => {
    win = null;
    pickerView = null;
    tabs.clear();
  });
  active = SERVERS;
  layout();
}

const SESSION_CONTINUES =
  "The session continues on the host until you log out, or for 8 hours with no connection. Connect again to get the desktop back.";

// The certificate of a server

/** The questions about a new pin that are open, by server id. Each request of the session waits for the same answer. */
const pinQuestions = new Map<string, Promise<boolean>>();

async function askPin(server: Server, pin: string): Promise<boolean> {
  const options = {
    type: "question" as const,
    buttons: ["Trust and connect", "Cancel"],
    defaultId: 1,
    cancelId: 1,
    title: "New server key",
    message: `This is the first connection to ${server.name}. Do you trust the key of this server?`,
    detail:
      `${server.url}\n\nSHA-256 of the public key:\n${pin}\n\n` +
      `To compare, run this command on the host. It must print the same text:\n${HOST_PIN_COMMAND}\n\n` +
      "The client keeps the key. If the key changes later, the client does not connect.",
  };
  const { response } = win ? await dialog.showMessageBox(win, options) : await dialog.showMessageBox(options);
  return response === 0;
}

/**
 * Lets the session of `server` accept the certificate of the server if its public key has the pin.
 * Certificates that Chromium accepts, for example from a public CA, need no pin.
 * A refusal goes to `refuse`.
 */
function pinCertificates(ses: Session, server: Server, refuse: (why: Refusal) => void): void {
  const host = new URL(server.url).hostname.replace(/^\[|\]$/g, "");
  ses.setCertificateVerifyProc((request, callback) => {
    if (request.errorCode === 0 || request.hostname.replace(/^\[|\]$/g, "") !== host) {
      callback(-3);
      return;
    }
    let pin: string;
    try {
      pin = publicKeyPin(request.certificate.data);
    } catch {
      callback(-2);
      return;
    }
    const kept = store.get(server.id)?.pin;
    const state = checkPin(kept, pin);
    if (state === "match") {
      callback(0);
    } else if (state === "changed") {
      refuse({ kind: "changed", kept: kept ?? "", now: pin });
      callback(-2);
    } else {
      let question = pinQuestions.get(server.id);
      if (!question) {
        question = askPin(server, pin).finally(() => pinQuestions.delete(server.id));
        pinQuestions.set(server.id, question);
      }
      void question.then((trusted) => {
        if (trusted) {
          store.setPin(server.id, pin);
          notifyPicker();
          callback(0);
        } else {
          refuse({ kind: "declined" });
          callback(-2);
        }
      });
    }
  });
}

/** Shows the server list with the cause of a connection that did not open. */
function showRefusal(server: Server, why: Refusal | null, description: string): void {
  if (why?.kind === "declined") {
    showServers(`The client did not connect to ${server.name}: you did not trust its key.`);
  } else if (why?.kind === "changed") {
    showServers(`The client did not connect to ${server.name}: the server key changed.`);
    if (win) {
      void dialog.showMessageBox(win, {
        type: "error",
        title: "The server key changed",
        message: `The key of ${server.name} is not the key of the first connection. The client does not connect.`,
        detail:
          `${server.url}\n\nKept key:\n${why.kept}\n\nKey now:\n${why.now}\n\n` +
          "An attacker can be between the client and the server. Ask the administrator of the host. " +
          "If the administrator made a new key, select Edit in the server list, then Forget the key.",
      });
    }
  } else {
    showServers(`${server.name} did not open: ${description}.`);
  }
}

// The shell of a server

/** Lets the shell get the permissions of {@link ALLOWED_PERMISSIONS}, only from the origin of `server`. */
function limitPermissions(ses: Session, server: Server): void {
  ses.setPermissionRequestHandler((_contents, permission, callback, details) => {
    callback(ALLOWED_PERMISSIONS.has(permission) && originOf(details.requestingUrl) === server.url);
  });
  ses.setPermissionCheckHandler((_contents, permission, requestingOrigin) => {
    return ALLOWED_PERMISSIONS.has(permission) && originOf(requestingOrigin) === server.url;
  });
}

/** Keeps the tab on the origin of `server`. Links to other places open in the browser of the desktop. */
function limitNavigation(contents: WebContents, server: Server): void {
  const guard = (event: { preventDefault(): void }, url: string) => {
    if (originOf(url) === server.url) return;
    event.preventDefault();
    openOutside(url);
  };
  contents.on("will-navigate", guard);
  contents.on("will-redirect", guard);
  contents.setWindowOpenHandler(({ url }) => {
    if (originOf(url) !== server.url) openOutside(url);
    return { action: "deny" };
  });
}

/** Removes the tab of `id` from the window. The shell does not get a beforeunload event. */
function dropTab(id: string): void {
  const tab = tabs.get(id);
  if (!tab) return;
  tabs.delete(id);
  if (!alive()) return;
  win!.contentView.removeChildView(tab.view);
  // After a close, the view has no webContents.
  const contents = tab.view.webContents as WebContents | undefined;
  if (contents && !contents.isDestroyed()) contents.close();
  if (active === id) {
    if (win?.isFullScreen()) win.setFullScreen(false);
    activate([...tabs.keys()].at(-1) ?? SERVERS);
  } else {
    publish();
  }
}

/** Closes the tab of `id`. The shell asks first, as in a browser. */
function closeTab(id: string): void {
  tabs.get(id)?.view.webContents.close({ waitForBeforeUnload: true });
}

/** Opens the shell of the server `id` in a new tab, or selects its tab. */
function connect(id: string): void {
  const server = store.get(id);
  if (!server) throw new Error("The server is not in the list.");
  showWindow();
  if (tabs.has(id)) {
    activate(id);
    return;
  }
  const ses = session.fromPartition(`persist:server-${server.id}`);
  const view = new WebContentsView({
    webPreferences: {
      session: ses,
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: false,
    },
  });
  view.setBackgroundColor(BACKGROUND);
  const tab: Tab = { server, view, title: "", refusal: null };
  tabs.set(id, tab);
  win!.contentView.addChildView(view);

  const contents = view.webContents;
  pinCertificates(ses, server, (why) => {
    tab.refusal ??= why;
  });
  limitPermissions(ses, server);
  limitNavigation(contents, server);
  addShortcuts(contents);
  contents.on("page-title-updated", (_event, title) => {
    tab.title = title === "farglass" ? "" : title;
    publish();
  });
  // The shell asks before a close or a reload. Electron does not ask, it only stops the close.
  // The question does not block the main process, thus the other tabs continue.
  let asking = false;
  contents.on("will-prevent-unload", (event) => {
    if (quitting || !win) {
      event.preventDefault();
      return;
    }
    if (asking) return;
    asking = true;
    void dialog
      .showMessageBox(win, {
        type: "question",
        buttons: ["Close the tab", "Stay"],
        defaultId: 1,
        cancelId: 1,
        title: "Close the desktop?",
        message: `Close the tab of ${server.name}?`,
        detail: SESSION_CONTINUES,
      })
      .then(({ response }) => {
        asking = false;
        if (response === 0) dropTab(id);
      });
  });
  contents.on("destroyed", () => dropTab(id));
  contents.on("did-fail-load", (_event, code, description, _url, isMainFrame) => {
    // -3 is ERR_ABORTED: a navigation that the client stopped.
    if (!isMainFrame || code === -3) return;
    const why = tab.refusal;
    dropTab(id);
    showRefusal(server, why, description);
  });

  store.touch(id);
  notifyPicker();
  activate(id);
  void contents.loadURL(`${server.url}/`);
}

// The tray

function updateTray(): void {
  if (!tray) return;
  const items: MenuItemConstructorOptions[] = [
    { label: "Show farglass", click: showWindow },
    { label: "Server list", click: () => showServers() },
  ];
  if (tabs.size > 0) {
    items.push({ type: "separator" });
    for (const [id, tab] of tabs) {
      items.push({
        label: tab.server.name,
        type: "radio",
        checked: id === active,
        click: () => {
          showWindow();
          activate(id);
        },
      });
    }
  }
  items.push(
    { type: "separator" },
    {
      label: "Close to the tray",
      type: "checkbox",
      checked: store.closeToTray,
      click: (item) => {
        store.closeToTray = item.checked;
        notifyPicker();
      },
    },
    { label: "Quit farglass", click: quit },
  );
  tray.setContextMenu(Menu.buildFromTemplate(items));
  tray.setToolTip(tabs.size === 0 ? "farglass" : `farglass: ${[...tabs.values()].map((t) => t.server.name).join(", ")}`);
}

function createTray(): void {
  const icon = nativeImage.createFromPath(staticFile("tray.png"));
  tray = new Tray(icon);
  tray.on("click", toggleWindow);
  updateTray();
}

function quit(): void {
  quitting = true;
  app.quit();
}

// IPC

type Reply<T> = { ok: true; value: T } | { ok: false; error: string };

/** Registers an IPC handler that only the page `page` can call, and that gives errors as a value. */
function handle<A extends unknown[], T>(page: "index.html" | "tabs.html", channel: string, fn: (...args: A) => T): void {
  ipcMain.handle(channel, (event: IpcMainInvokeEvent, ...args: unknown[]): Reply<T> => {
    const contents = page === "index.html" ? pickerView?.webContents : win?.webContents;
    if (!contents || event.sender !== contents || event.senderFrame?.url !== fileUrl(page)) {
      return { ok: false, error: "Refused." };
    }
    try {
      return { ok: true, value: fn(...(args as A)) };
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
  });
}

const input = (value: unknown): ServerInput => {
  const v = (value ?? {}) as Record<string, unknown>;
  return { name: String(v["name"] ?? ""), url: String(v["url"] ?? "") };
};

function registerIpc(): void {
  handle("index.html", "servers:list", () => store.list());
  handle("index.html", "servers:add", (value: unknown) => store.add(input(value)));
  handle("index.html", "servers:update", (id: unknown, value: unknown) => {
    const server = store.update(String(id), input(value));
    const tab = tabs.get(server.id);
    if (tab) {
      tab.server = server;
      publish();
    }
    return server;
  });
  handle("index.html", "servers:remove", (id: unknown) => {
    dropTab(String(id));
    store.remove(String(id));
  });
  handle("index.html", "servers:forget-pin", (id: unknown) => store.forgetPin(String(id)));
  handle("index.html", "servers:connect", (id: unknown) => connect(String(id)));
  handle("index.html", "settings:get", () => ({ closeToTray: store.closeToTray }));
  handle("index.html", "settings:close-to-tray", (value: unknown) => {
    store.closeToTray = value === true;
    updateTray();
  });
  handle("index.html", "app:version", () => app.getVersion());
  handle("tabs.html", "tabs:select", (id: unknown) => activate(String(id)));
  handle("tabs.html", "tabs:close", (id: unknown) => closeTab(String(id)));
}

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on("second-instance", showWindow);
  // No other code runs in the window: no webview, and no window that a page opens with its own preferences.
  app.on("web-contents-created", (_event, contents) => {
    contents.on("will-attach-webview", (event) => event.preventDefault());
  });
  app.on("before-quit", () => {
    quitting = true;
  });
  app.on("window-all-closed", () => app.quit());
  void app.whenReady().then(() => {
    // No menu: its shortcuts (Ctrl+W, Ctrl+R, Ctrl+Q) belong to the apps of the host.
    Menu.setApplicationMenu(null);
    store = new ServerStore(join(app.getPath("userData"), "servers.json"));
    registerIpc();
    createTray();
    createWindow();
  });
}
