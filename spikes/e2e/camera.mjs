// camera.mjs - end-to-end test of the camera for the apps (F116)
//
// Usage: npm --prefix spikes/e2e run camera
//
// The script starts farglassd in dev_user mode and the agent, which adds its
// camera node to the PipeWire server of the user. The client Chromium runs
// with its fake camera. A click on the Camera button arms the camera, and
// the browser must not capture yet. Then a host Chromium opens the camera
// through the camera portal, as an app of the web session does: it runs with
// a private D-Bus session and portal (spikes/camera/portal.sh), so the test
// answers no Access dialog. It must see the node of the agent as its only
// camera, and get pictures that are not black. While it reads, the panel
// shows Cam on. When it stops, the panel shows Cam ready again.
//
// GSettings stays in memory for the agent and for the host Chromium, so the
// camera switch of the privacy settings of the user does not apply.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test with no PipeWire, no D-Bus session tools, or no desktop portal.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let failed = false;

/** Stops `child` and all processes of its group: portal.sh stops its private bus and portal then. */
function stopGroup(child, signal = "SIGTERM") {
  try {
    process.kill(-child.pid, signal);
  } catch {
    // The child leads no group of its own, or the group ended.
  }
  child.kill(signal);
}

/** Returns the number of PipeWire nodes with the name farglass-camera: the camera node of each running agent. */
function cameraNodes() {
  const nodes = JSON.parse(execFileSync("pw-dump", { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }));
  return nodes.filter((o) => o.type === "PipeWire:Interface:Node" && o.info?.props?.["node.name"] === "farglass-camera").length;
}

// An interrupted run stops its children too: a camera node that stays confuses the next run.
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    for (const child of children) stopGroup(child, "SIGKILL");
    process.exit(1);
  });
}

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

/** Runs the host Chromium on the camera portal. Resolves with its result line and its exit status. */
function hostChromium() {
  return new Promise((resolve) => {
    const child = spawn(
      "../camera/portal.sh",
      ["farglass", "node", "../camera/browser.mjs", "chromium", "farglass browser camera", "--enable-features=WebRtcPipeWireCamera"],
      { cwd: import.meta.dirname, env: { ...process.env, GRANT: "yes" }, stdio: ["ignore", "pipe", "ignore"], detached: true },
    );
    children.push(child);
    let out = "";
    child.stdout.on("data", (chunk) => (out += chunk));
    child.on("close", (status) => {
      const line = out.split("\n").find((l) => l.startsWith("{")) ?? out.trim();
      resolve({ status, line });
    });
  });
}

try {
  try {
    execFileSync("pw-cli", ["info", "0"], { stdio: "ignore" });
    execFileSync("dbus-run-session", ["--", "true"], { stdio: "ignore" });
  } catch {
    console.log("SKIP: no PipeWire, or no dbus-run-session");
    process.exit(0);
  }
  if (!existsSync("/usr/libexec/xdg-desktop-portal")) {
    console.log("SKIP: no xdg-desktop-portal");
    process.exit(0);
  }
  if (cameraNodes() > 0) {
    throw new Error("A different agent has a farglass-camera node: stop it first, else the host Chromium can read that node.");
  }
  buildAll();
  const socket = join(state, "agent.sock");
  const wayland = join(state, "wayland-0");
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
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, GSETTINGS_BACKEND: "memory" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;
  check(/the camera node of the session is in PipeWire/.test(agent.log()), "the agent adds its camera node to PipeWire");

  browser = await chromium.launch({ args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"] });
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1200, height: 800 } });
  await context.grantPermissions(["camera"], { origin: url });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  const button = page.locator(".panel-camera");
  await button.waitFor({ timeout: TIMEOUT_MS });
  check((await button.textContent()) === "Cam off", "the panel shows the camera button, off");

  await button.click();
  await page.waitForFunction(() => document.querySelector(".panel-camera")?.textContent === "Cam ready", null, { timeout: TIMEOUT_MS });
  check(/event=camera result=accepted reason=arm/.test(agent.log()), "a click arms the camera, and the agent logs it");
  await page.waitForTimeout(500);
  check(!/reason=start/.test(agent.log()), "the browser does not capture while no app of the host reads the camera");

  const host = hostChromium();
  await page.waitForFunction(() => document.querySelector(".panel-camera")?.dataset.state === "on", null, { timeout: 60_000 });
  check(true, "the panel shows Cam on while the host Chromium reads the camera");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "camera.png") });
  const { status, line } = await host;
  const result = JSON.parse(line.startsWith("{") ? line : "{}");
  check(
    status === 0 && result.labels?.length === 1,
    `the host Chromium sees the camera of the browser as its only camera, and gets pictures that are not black: ${line}`,
  );
  check(/event=camera result=accepted reason=start/.test(agent.log()), "the agent logs the start of the stream");

  await page.waitForFunction(() => document.querySelector(".panel-camera")?.textContent === "Cam ready", null, { timeout: TIMEOUT_MS });
  check(/event=camera result=accepted reason=stop/.test(agent.log()), "the host Chromium stops, the capture stops, and the camera stays armed");

  await button.click();
  check((await button.textContent()) === "Cam off", "a second click turns the camera off");
  await page.waitForTimeout(300);
  check(/event=camera result=accepted reason=disarm/.test(agent.log()), "the agent logs the disarm");
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) stopGroup(child, "SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
