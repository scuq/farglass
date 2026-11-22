// browsers.mjs - end-to-end test of the shell in Firefox and WebKit (F036)
//
// Usage: npm --prefix spikes/e2e run browsers
//        BROWSERS=webkit npm --prefix spikes/e2e run browsers
//
// The script starts farglass-login with a PAM directory that needs no root
// (pam_exec accepts the password "secret"), farglassd with no dev_user, and
// the agent with a Wayland socket. For each browser of BROWSERS (default:
// firefox and webkit), it logs in, uses the echo channel and a terminal,
// moves a window, changes a setting, shows a window of gnome-calculator and
// types in it, and logs out. It prints the APIs that the browser does not have.
//
// Exit status: 0 if all steps pass in each browser, 1 if a step fails.

import { spawn } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import { firefox, webkit } from "playwright";
import { PRIMARY_GROUP, buildAll, echo, launch, logout, raise, start, TIMEOUT_MS } from "./lib.mjs";

const ENGINES = { firefox, webkit };
const NAMES = (process.env.BROWSERS ?? "firefox,webkit").split(",").map((name) => name.trim());
const CALCULATOR = "/usr/bin/gnome-calculator";
const FIXTURE = join(import.meta.dirname, "fixtures", "animation.py");
const SHOTS = process.env.FARGLASS_SHOTS;
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let failed = false;

/** Writes a PAM directory with a stack that accepts the password "secret". */
function pamDirectory() {
  const dir = join(state, "pam.d");
  mkdirSync(dir);
  const script = join(state, "check.sh");
  writeFileSync(script, '#!/bin/sh\nIFS= read -r password; [ "$password" = secret ]\n');
  chmodSync(script, 0o755);
  writeFileSync(join(dir, "farglass"), `auth required pam_exec.so expose_authtok quiet ${script}\naccount required pam_permit.so\n`);
  return dir;
}

/** Answers the prompt with the label `label` on the login screen. */
async function answer(page, label, text) {
  const field = page.getByLabel(label, { exact: true });
  await field.waitFor({ timeout: TIMEOUT_MS });
  await field.fill(text);
  await page.getByRole("button", { name: "Continue" }).click();
}

/** Runs the steps in one browser. */
async function run(name, url, user, wayland) {
  const log = (text) => console.log(`PASS: ${name}: ${text}`);
  const browser = await ENGINES[name].launch();
  let page;
  try {
    const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 860 } });
    page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error" || message.text().includes("Content Security Policy")) errors.push(message.text());
    });

    // 1. The login.
    await page.goto(url);
    await page.getByRole("heading", { name: /Log in to/ }).waitFor({ timeout: TIMEOUT_MS });
    await answer(page, "Login", user);
    await answer(page, "Password", "secret");
    await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
    const apis = await page.evaluate(() => ({
      WebTransport: typeof WebTransport === "function",
      VideoDecoder: typeof VideoDecoder === "function",
      AudioDecoder: typeof AudioDecoder === "function",
      "Keyboard Lock": !!navigator.keyboard?.lock,
      "Window Management": typeof window.getScreenDetails === "function",
      "clipboard.read": typeof navigator.clipboard?.read === "function",
      PublicKeyCredential: typeof PublicKeyCredential === "function",
    }));
    const missing = Object.entries(apis)
      .filter(([, has]) => !has)
      .map(([api]) => api);
    console.log(`INFO: ${name}: the browser has no ${missing.join(", ") || "missing API"}`);
    log("the password logs in");

    // 2. The echo channel, and a reload keeps the session.
    const time = await echo(page, `hello from ${name}`);
    log(`the echo channel works, round trip ${time} ms`);
    await page.reload();
    await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
    log("a reload keeps the session through the cookie");

    // 3. A terminal runs a command.
    await launch(page, "Terminal");
    const terminal = page.locator(".wm-window").filter({ has: page.locator('.term-app[data-status="running"]') }).last();
    await terminal.waitFor({ timeout: TIMEOUT_MS });
    await terminal.locator(".term-app").click();
    await page.keyboard.type("echo farglass-$((40+2))");
    await page.keyboard.press("Enter");
    const term = terminal.locator(".term-app");
    const end = Date.now() + TIMEOUT_MS;
    while (!(await term.evaluate((el) => el.farglassTerminal.text())).includes("farglass-42")) {
      if (Date.now() > end) throw new Error(`${name}: the terminal does not show the output of the command`);
      await page.waitForTimeout(100);
    }
    log("a terminal runs a command, and its output shows");

    // 4. A drag of the title bar moves the window.
    const bar = terminal.locator(".wm-titlebar");
    const before = await terminal.boundingBox();
    const grip = await bar.boundingBox();
    await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
    await page.mouse.down();
    await page.mouse.move(grip.x + grip.width / 2 + 120, grip.y + grip.height / 2 + 60, { steps: 8 });
    await page.mouse.up();
    const after = await terminal.boundingBox();
    if (Math.abs(after.x - before.x - 120) > 4 || Math.abs(after.y - before.y - 60) > 4) {
      throw new Error(`${name}: the window moved from ${before.x},${before.y} to ${after.x},${after.y}`);
    }
    log("a drag of the title bar moves the window");

    // 5. A setting applies at once.
    // A Settings window of a run before can come back from the state store: close it first.
    const old = page.locator('.wm-window[aria-label^="Settings"]');
    while ((await old.count()) > 0) await old.first().getByRole("button", { name: "Close" }).click();
    await launch(page, "Settings");
    const settings = page.locator('.wm-window[aria-label="Settings"]');
    await settings.getByRole("group", { name: "Theme" }).getByLabel("Dark").check();
    await page.waitForFunction(() => document.documentElement.dataset.theme === "dark", null, { timeout: TIMEOUT_MS });
    await settings.getByRole("group", { name: "Theme" }).getByLabel("Same as this computer").check();
    log("the dark theme applies at once");

    // 6. A Linux app shows and gets keys.
    if (existsSync(CALCULATOR)) {
      const app = spawn(CALCULATOR, [], {
        env: { ...process.env, WAYLAND_DISPLAY: wayland, GDK_BACKEND: "wayland", DBUS_SESSION_BUS_ADDRESS: "disabled:", DISPLAY: "" },
        stdio: "ignore",
      });
      try {
        const window = page.locator(".wm-window").filter({ has: page.locator(".linux-app") });
        await window.waitFor({ timeout: 30_000 });
        await raise(page, window);
        const canvas = window.locator(".linux-app canvas").first();
        await page.waitForFunction(() => document.querySelector(".linux-app canvas")?.style.width, null, { timeout: TIMEOUT_MS });
        await page.waitForTimeout(500);
        const shot = async () => (await canvas.screenshot()).toString("base64");
        const initial = await shot();
        await window.locator(".wm-titlebar").click({ position: { x: 150, y: 10 } });
        for (const key of ["7", "Shift+Equal", "2", "Enter"]) await page.keyboard.press(key);
        let typed = initial;
        for (let i = 0; i < 40 && typed === initial; i++) {
          await page.waitForTimeout(100);
          typed = await shot();
        }
        if (typed === initial) throw new Error(`${name}: the keys did not reach the Linux app`);
        if (SHOTS) await page.screenshot({ path: join(SHOTS, `browsers-${name}.png`) });
        log("a Linux app shows its window, and keys reach it");
        await window.getByRole("button", { name: "Close" }).click();
      } finally {
        app.kill("SIGTERM");
      }
    } else {
      console.log(`INFO: ${name}: no gnome-calculator, the Linux app step is not tested`);
    }

    // 6b. A region that changes in each frame goes as video, or as tiles if the browser cannot decode H.264.
    const animation = spawn("python3", [FIXTURE], {
      env: { ...process.env, WAYLAND_DISPLAY: wayland, GDK_BACKEND: "wayland", DBUS_SESSION_BUS_ADDRESS: "disabled:", DISPLAY: "" },
      stdio: "ignore",
    });
    try {
      const window = page.locator('.wm-window[aria-label="Animation"]');
      await window.waitFor({ timeout: 60_000 });
      await raise(page, window);
      const canvas = window.locator("canvas").first();
      await page.waitForTimeout(3000);
      const frames = Number((await canvas.getAttribute("data-video-frames")) ?? 0);
      const first = (await canvas.screenshot()).toString("base64");
      await page.waitForTimeout(500);
      const second = (await canvas.screenshot()).toString("base64");
      if (first === second) throw new Error(`${name}: the animation does not move`);
      console.log(`INFO: ${name}: the moving region goes ${frames > 0 ? `as H.264 video (${frames} frames decoded)` : "as tiles: the browser cannot decode the H.264 video"}`);
      log("an animation of a Linux app moves in the window");
      await window.getByRole("button", { name: "Close" }).click();
    } finally {
      animation.kill("SIGTERM");
    }

    // 7. The logout. The state store keeps the windows for the next login: close them first.
    await settings.getByRole("button", { name: "Close" }).click();
    await terminal.getByRole("button", { name: "Close" }).click();
    await logout(page, user);
    await page.getByText("You logged out.").waitFor({ timeout: TIMEOUT_MS });
    log("the logout works");
    const real = errors.filter((text) => !/WebTransport|ERR_QUIC|net::|Failed to load resource/.test(text));
    if (real.length > 0) throw new Error(`${name}: page errors: ${real.join("; ")}`);
    log("no page error and no Content Security Policy violation");
  } catch (error) {
    if (SHOTS && page) await page.screenshot({ path: join(SHOTS, `browsers-${name}-failed.png`) }).catch(() => {});
    throw error;
  } finally {
    await browser.close();
  }
}

try {
  buildAll();
  const user = userInfo().username;
  const loginSocket = join(state, "login.sock");
  const agentSocket = join(state, "agent.sock");
  const wayland = join(state, "wayland-0");
  const login = start(
    "farglass-login",
    "target/debug/farglass-login",
    ["--socket", loginSocket, "--pam-dir", pamDirectory(), "--fail-delay-ms", "0", "--session", "none", "--allowed-group", PRIMARY_GROUP],
    {},
    /ready/,
  );
  children.push(login.child);
  await login.match;
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", agentSocket, "--login-socket", loginSocket, "--allow-user-agents"],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, direct] = await daemon.match;
  const url = `https://localhost:${new URL(direct).port}/`;
  const agent = start(
    "farglass-session",
    "target/debug/farglass-session",
    [],
    { FARGLASS_AGENT_SOCKET: agentSocket, FARGLASS_WAYLAND_SOCKET: wayland },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  for (const name of NAMES) {
    if (!ENGINES[name]) throw new Error(`unknown browser ${name}`);
    try {
      await run(name, url, user, wayland);
    } catch (error) {
      failed = true;
      console.error(`FAIL: ${error.message}`);
    }
  }
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
