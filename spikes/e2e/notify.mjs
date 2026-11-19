// notify.mjs - end-to-end test of the desktop notifications in Chromium
//
// Usage: npm --prefix spikes/e2e run notify
//
// The script starts farglassd in dev_user mode and the agent. The agent serves
// the notifications under a test name on the session bus, because the desktop
// of the host can own org.freedesktop.Notifications. gdbus sends a notification
// before the shell opens, and the script checks the toast, an action, a
// dismiss, and CloseNotification. gdbus monitor records the signals.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const NAME = `org.farglass.e2e.Notifications${process.pid}`;
const PATH = "/org/freedesktop/Notifications";
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let failed = false;

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

/** Calls a method of the notification service and returns the output of gdbus. */
function call(method, ...args) {
  return execFileSync(
    "gdbus",
    ["call", "--session", "--dest", NAME, "--object-path", PATH, "--method", `org.freedesktop.Notifications.${method}`, "--", ...args],
    { encoding: "utf8" },
  ).trim();
}

/** Returns `text` as a GVariant string in the text form of gdbus. */
const str = (text) => `'${text.replaceAll("\\", "\\\\").replaceAll("'", "\\'").replaceAll("\n", "\\n")}'`;

/** Sends a notification and returns its id. */
function notify(summary, body, actions, urgency, timeout = -1) {
  const list = `[${actions.map(str).join(", ")}]`;
  const out = call("Notify", str("farglass-e2e"), "0", str(""), str(summary), str(body), list, `{'urgency': <byte ${urgency}>}`, String(timeout));
  return Number(out.match(/uint32 (\d+)/)[1]);
}

/** Waits until the output of `monitor` contains `text`. */
async function signal(monitor, text) {
  const end = Date.now() + TIMEOUT_MS;
  while (!monitor.out.includes(text)) {
    if (Date.now() > end) throw new Error(`no signal ${text} in:\n${monitor.out}`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

try {
  buildAll();
  const socket = join(state, "agent.sock");
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start(
    "farglass-session",
    "target/debug/farglass-session",
    [],
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_NOTIFICATIONS_NAME: NAME },
    /serving the desktop notifications/,
  );
  children.push(agent.child);
  await agent.match;

  const monitor = { out: "" };
  const child = spawn("gdbus", ["monitor", "--session", "--dest", NAME, "--object-path", PATH], { stdio: ["ignore", "pipe", "inherit"] });
  child.stdout.on("data", (chunk) => {
    monitor.out += chunk;
  });
  children.push(child);
  await new Promise((resolve) => setTimeout(resolve, 300));

  const info = call("GetServerInformation");
  check(info.startsWith("('farglass', 'farglass'") && info.includes("'1.2'"), "GetServerInformation names farglass and the specification 1.2");
  check(call("GetCapabilities").includes("'actions'"), "GetCapabilities gives actions");

  // 1. A notification before the shell connects waits in the agent.
  const mail = notify("New mail", "From Alice\nAbout the backup", ["default", "Open", "later", "Later"], 1);

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  await page.goto(url);
  const toast = page.locator('.notification[aria-label="New mail"]');
  await toast.waitFor({ timeout: TIMEOUT_MS });
  check((await toast.locator(".notification-app").textContent()) === "farglass-e2e", "the notification from before the shell connected shows with its program");
  const toastBox = await toast.boundingBox();
  check(toastBox.x + toastBox.width > 1200 && toastBox.y < 120, "the notification is at the top right");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "notify.png") });

  // 2. An action button sends ActionInvoked, then NotificationClosed with reason 2.
  await toast.hover();
  await toast.getByRole("button", { name: "Later" }).click();
  await signal(monitor, `ActionInvoked (uint32 ${mail}, 'later')`);
  await signal(monitor, `NotificationClosed (uint32 ${mail}, uint32 2)`);
  await toast.waitFor({ state: "detached", timeout: TIMEOUT_MS });
  check(true, "an action button sends ActionInvoked and closes the notification");

  // 3. A critical notification stays until the user dismisses it.
  const alert = notify("Disk full", "The disk of /home is full.", [], 2);
  const alertToast = page.locator('.notification[aria-label="Disk full"]');
  await alertToast.waitFor({ timeout: TIMEOUT_MS });
  check((await alertToast.getAttribute("role")) === "alert", "a critical notification is an alert");
  await page.mouse.move(600, 600);
  await page.waitForTimeout(7000);
  check(await alertToast.isVisible(), "a critical notification stays after the usual time");
  await alertToast.getByRole("button", { name: "Dismiss" }).click();
  await signal(monitor, `NotificationClosed (uint32 ${alert}, uint32 2)`);
  check(true, "Dismiss sends NotificationClosed with reason 2");

  // 4. The pointer on a notification pauses its time.
  const paused = notify("Paused", "", [], 1, 1500);
  const pausedToast = page.locator('.notification[aria-label="Paused"]');
  await pausedToast.waitFor({ timeout: TIMEOUT_MS });
  await pausedToast.hover();
  await page.waitForTimeout(2500);
  check(await pausedToast.isVisible(), "the pointer on a notification pauses its time");
  await page.mouse.move(600, 600);
  await signal(monitor, `NotificationClosed (uint32 ${paused}, uint32 1)`);
  check(true, "the time continues when the pointer goes");

  // 5. The time to show ends with reason 1, and CloseNotification closes with reason 3.
  const short = notify("Short", "", [], 1, 1500);
  await signal(monitor, `NotificationClosed (uint32 ${short}, uint32 1)`);
  check(await page.locator('.notification[aria-label="Short"]').count() === 0, "the time to show ends with reason 1");
  const kept = notify("Kept", "", [], 1, 0);
  await page.locator('.notification[aria-label="Kept"]').waitFor({ timeout: TIMEOUT_MS });
  call("CloseNotification", String(kept));
  await page.locator('.notification[aria-label="Kept"]').waitFor({ state: "detached", timeout: TIMEOUT_MS });
  await signal(monitor, `NotificationClosed (uint32 ${kept}, uint32 3)`);
  check(true, "CloseNotification removes the notification with reason 3");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
