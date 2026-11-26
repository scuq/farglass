/**
 * The main process of the desktop client of farglass (F121, ADR-060).
 *
 * At the start, the client shows the list of servers. A connection opens a
 * window that loads the shell from farglassd. The window has the sandbox, no
 * preload, and its own session for each server. A self-signed certificate is
 * accepted only if the SHA-256 of its public key is the pin of the server.
 * The user accepts the pin at the first connection.
 */
import { BrowserWindow, Menu, app, dialog, ipcMain, session, shell } from "electron";
import type { IpcMainInvokeEvent, Session, WebContents } from "electron";
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

/** The background of the windows while the page loads: the dark background of the shell. */
const BACKGROUND = "#000000";

let store: ServerStore;
let picker: BrowserWindow | null = null;
const sessions = new Map<string, BrowserWindow>();

const pickerPage = () => join(__dirname, "..", "static", "index.html");

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

function showPicker(): void {
  if (picker && !picker.isDestroyed()) {
    picker.show();
    picker.focus();
    return;
  }
  picker = new BrowserWindow({
    width: 720,
    height: 560,
    minWidth: 480,
    minHeight: 360,
    title: "farglass",
    backgroundColor: BACKGROUND,
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, "preload.js"),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  picker.on("closed", () => {
    picker = null;
  });
  void picker.loadFile(pickerPage());
}

/** Tells the picker that the list changed, or shows a message in it. */
function notifyPicker(message?: string): void {
  if (picker && !picker.isDestroyed()) picker.webContents.send("servers:changed", message ?? null);
}

/** The questions about a new pin that are open, by server id. Each request of the session waits for the same answer. */
const pinQuestions = new Map<string, Promise<boolean>>();

async function askPin(server: Server, pin: string, parent: BrowserWindow | null): Promise<boolean> {
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
  const { response } = parent ? await dialog.showMessageBox(parent, options) : await dialog.showMessageBox(options);
  return response === 0;
}

/** Why the client refused the certificate of a server: the user said no, or the key changed. */
type Refusal = { kind: "declined" } | { kind: "changed"; kept: string; now: string };

/**
 * Lets the session of `server` accept the certificate of the server if its public key has the pin.
 * Certificates that Chromium accepts, for example from a public CA, need no pin.
 * A refusal goes to `refuse`, once for each window.
 */
function pinCertificates(ses: Session, server: Server, win: BrowserWindow, refuse: (why: Refusal) => void): void {
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
        question = askPin(server, pin, win).finally(() => pinQuestions.delete(server.id));
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

/** Closes the window of `server` and tells the user in the server list why. */
function showRefusal(server: Server, why: Refusal | null, description: string): void {
  showPicker();
  if (why?.kind === "declined") {
    notifyPicker(`The client did not connect to ${server.name}: you did not trust its key.`);
  } else if (why?.kind === "changed") {
    notifyPicker(`The client did not connect to ${server.name}: the server key changed.`);
    void dialog.showMessageBox(picker!, {
      type: "error",
      title: "The server key changed",
      message: `The key of ${server.name} is not the key of the first connection. The client does not connect.`,
      detail:
        `${server.url}\n\nKept key:\n${why.kept}\n\nKey now:\n${why.now}\n\n` +
        "An attacker can be between the client and the server. Ask the administrator of the host. " +
        "If the administrator made a new key, select Edit in the server list, then Forget the key.",
    });
  } else {
    notifyPicker(`${server.name} did not open: ${description}.`);
  }
}

/** Lets the shell get the permissions of {@link ALLOWED_PERMISSIONS}, only from the origin of `server`. */
function limitPermissions(ses: Session, server: Server): void {
  ses.setPermissionRequestHandler((_contents, permission, callback, details) => {
    callback(ALLOWED_PERMISSIONS.has(permission) && originOf(details.requestingUrl) === server.url);
  });
  ses.setPermissionCheckHandler((_contents, permission, requestingOrigin) => {
    return ALLOWED_PERMISSIONS.has(permission) && originOf(requestingOrigin) === server.url;
  });
}

/** Keeps the window on the origin of `server`. Links to other places open in the browser of the desktop. */
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

/** Ctrl+Shift+F12 shows the server list. The shell gets each other key. */
function addShortcuts(contents: WebContents): void {
  contents.on("before-input-event", (event, input) => {
    if (input.type === "keyDown" && input.control && input.shift && !input.alt && !input.meta && input.code === "F12") {
      event.preventDefault();
      showPicker();
    }
  });
}

function connect(id: string): void {
  const server = store.get(id);
  if (!server) throw new Error("The server is not in the list.");
  const open = sessions.get(id);
  if (open && !open.isDestroyed()) {
    open.show();
    open.focus();
    return;
  }
  const ses = session.fromPartition(`persist:server-${server.id}`);
  const win = new BrowserWindow({
    width: 1440,
    height: 900,
    title: server.name,
    backgroundColor: BACKGROUND,
    autoHideMenuBar: true,
    webPreferences: {
      session: ses,
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      webviewTag: false,
      spellcheck: false,
    },
  });
  sessions.set(id, win);
  win.on("closed", () => sessions.delete(id));
  win.on("page-title-updated", (event, title) => {
    event.preventDefault();
    win.setTitle(title && title !== "farglass" ? `${server.name} — ${title}` : server.name);
  });
  let refusal: Refusal | null = null;
  pinCertificates(ses, server, win, (why) => {
    refusal ??= why;
  });
  limitPermissions(ses, server);
  limitNavigation(win.webContents, server);
  addShortcuts(win.webContents);
  // The shell asks before a close or a reload. Electron does not ask, it only stops the close.
  win.webContents.on("will-prevent-unload", (event) => {
    const choice = dialog.showMessageBoxSync(win, {
      type: "question",
      buttons: ["Close the window", "Stay"],
      defaultId: 1,
      cancelId: 1,
      title: "Close the desktop?",
      message: `Close the window of ${server.name}?`,
      detail: "The session continues on the host until you log out, or for 8 hours with no connection. Connect again to get the desktop back.",
    });
    if (choice === 0) event.preventDefault();
  });
  win.webContents.on("did-fail-load", (_event, code, description, _url, isMainFrame) => {
    // -3 is ERR_ABORTED: a navigation that the client stopped.
    if (!isMainFrame || code === -3 || win.isDestroyed()) return;
    win.close();
    showRefusal(server, refusal, description);
  });
  store.touch(id);
  notifyPicker();
  void win.loadURL(`${server.url}/`);
}

type Reply<T> = { ok: true; value: T } | { ok: false; error: string };

/** Registers an IPC handler that only the picker page can call, and that gives errors as a value. */
function handle<A extends unknown[], T>(channel: string, fn: (...args: A) => T): void {
  ipcMain.handle(channel, (event: IpcMainInvokeEvent, ...args: unknown[]): Reply<T> => {
    if (!picker || event.sender !== picker.webContents || event.senderFrame?.url !== pathToFileURL(pickerPage()).href) {
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
  handle("servers:list", () => store.list());
  handle("servers:add", (value: unknown) => store.add(input(value)));
  handle("servers:update", (id: unknown, value: unknown) => store.update(String(id), input(value)));
  handle("servers:remove", (id: unknown) => {
    sessions.get(String(id))?.close();
    store.remove(String(id));
  });
  handle("servers:forget-pin", (id: unknown) => store.forgetPin(String(id)));
  handle("servers:connect", (id: unknown) => connect(String(id)));
  handle("app:version", () => app.getVersion());
}

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on("second-instance", showPicker);
  // No other code runs in a window: no webview, and no window that a page opens with its own preferences.
  app.on("web-contents-created", (_event, contents) => {
    contents.on("will-attach-webview", (event) => event.preventDefault());
  });
  app.on("window-all-closed", () => app.quit());
  void app.whenReady().then(() => {
    // No menu: its shortcuts (Ctrl+W, Ctrl+R, Ctrl+Q) belong to the apps of the host.
    Menu.setApplicationMenu(null);
    store = new ServerStore(join(app.getPath("userData"), "servers.json"));
    registerIpc();
    showPicker();
  });
}
