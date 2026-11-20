// clipboard.mjs - end-to-end test of the clipboard between the Wayland apps and the browser
//
// Usage: npm --prefix spikes/e2e run clipboard
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket in a temporary directory. It starts gnome-calculator and VS Code on
// that socket. It writes text to the clipboard of Chromium and pastes it in
// VS Code, copies the text of VS Code back to Chromium, and copies a result
// of the calculator to Chromium and to VS Code (F030).
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test if gnome-calculator or VS Code is not installed.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, start, TIMEOUT_MS, raise } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const CALCULATOR = "/usr/bin/gnome-calculator";
const CODE = "/usr/share/code/code";
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

/** Waits until `predicate` is true, for `ms` at most. */
async function until(page, predicate, ms, text) {
  const end = Date.now() + ms;
  while (!(await predicate())) {
    if (Date.now() > end) throw new Error(`timeout: ${text}`);
    await page.waitForTimeout(100);
  }
}

try {
  if (!existsSync(CALCULATOR) || !existsSync(CODE)) {
    console.log("SKIP: gnome-calculator or VS Code is not installed");
    process.exit(0);
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
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1600, height: 1000 } });
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: new URL(url).origin });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  // A clean environment: this test can run inside VS Code, whose variables would start VS Code
  // in its Node mode or open the window in the running VS Code.
  const env = {
    HOME: process.env.HOME,
    PATH: "/usr/bin:/bin",
    XDG_RUNTIME_DIR: process.env.XDG_RUNTIME_DIR,
    WAYLAND_DISPLAY: wayland,
    XDG_SESSION_TYPE: "wayland",
    GDK_BACKEND: "wayland",
    DBUS_SESSION_BUS_ADDRESS: "disabled:",
  };
  // A new profile of VS Code: no sign-in dialog and no welcome page over the window.
  mkdirSync(join(state, "code", "User"), { recursive: true });
  writeFileSync(
    join(state, "code", "User", "settings.json"),
    JSON.stringify({ "workbench.startupEditor": "none", "chat.disableAIFeatures": true, "workbench.tips.enabled": false, "update.mode": "none", "telemetry.telemetryLevel": "off" }),
  );
  children.push(spawn(CALCULATOR, [], { env, stdio: "ignore" }));
  children.push(
    spawn(CODE, ["--ozone-platform=wayland", `--user-data-dir=${join(state, "code")}`, `--extensions-dir=${join(state, "ext")}`, "--disable-workspace-trust", "--new-window"], {
      env,
      stdio: "ignore",
    }),
  );
  const calculator = page.locator('.wm-window[aria-label="Calculator"]');
  const code = page.locator(".wm-window").filter({ has: page.locator(".linux-app") }).filter({ hasText: "" }).and(page.locator('.wm-window:not([aria-label="Calculator"])'));
  await calculator.waitFor({ timeout: 60_000 });
  await code.waitFor({ timeout: 60_000 });
  await until(page, async () => /Visual Studio Code/.test((await code.getAttribute("aria-label")) ?? ""), 60_000, "VS Code shows its window");
  await page.waitForTimeout(2000);
  await raise(page, calculator);
  await raise(page, code);
  check(true, "gnome-calculator and VS Code show their windows");

  const title = async () => (await code.getAttribute("aria-label")) ?? "";
  const clickIn = async (window, x, y) => {
    const box = await window.locator(".linux-app").boundingBox();
    await page.mouse.click(box.x + x, box.y + y);
    await page.waitForTimeout(300);
  };
  const browserClipboard = () => page.evaluate(() => navigator.clipboard.readText());
  // VS Code is right of the calculator: the windows open at the same place.
  const codeBox = await code.locator(".linux-app").boundingBox();
  await page.mouse.move(codeBox.x + codeBox.width - 220, codeBox.y + 17);
  await page.mouse.down();
  await page.waitForTimeout(300);
  await page.mouse.move(codeBox.x + codeBox.width + 280, codeBox.y + 47, { steps: 12 });
  await page.mouse.up();
  await page.waitForTimeout(500);

  // 1. A copy in the browser: a paste in VS Code gets it.
  await page.evaluate(() => navigator.clipboard.writeText("pasted from the browser"));
  await clickIn(code, 400, 300);
  await page.keyboard.press("Control+N");
  await page.waitForTimeout(1500);
  await page.keyboard.press("Control+V");
  await until(page, async () => (await title()).startsWith("● pasted from the browser"), TIMEOUT_MS, "VS Code pastes the browser clipboard");
  check(true, `Ctrl+V in VS Code pastes the clipboard of the browser: ${await title()}`);

  // 2. A copy in VS Code: the browser gets it.
  await page.keyboard.type(" and back", { delay: 30 });
  await page.keyboard.press("Control+A");
  await page.keyboard.press("Control+C");
  await until(page, async () => (await browserClipboard()) === "pasted from the browser and back", TIMEOUT_MS, "the browser gets the copy of VS Code");
  check(true, "Ctrl+C in VS Code sets the clipboard of the browser");

  // 3. A copy in the calculator: the browser and VS Code get it.
  await clickIn(calculator, 150, 320);
  await page.keyboard.type("6", { delay: 50 });
  await page.keyboard.press("Shift+Digit8");
  await page.keyboard.type("7", { delay: 50 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(500);
  await page.keyboard.press("Control+C");
  await until(page, async () => (await browserClipboard()) === "42", TIMEOUT_MS, "the browser gets the result of the calculator");
  check(true, "Ctrl+C in the calculator sets the clipboard of the browser to 42");
  await clickIn(code, 400, 300);
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Control+V");
  // VS Code reads the selection of the calculator through a pipe: the next keys wait for the paste.
  await page.waitForTimeout(1000);
  await page.keyboard.press("Control+Home");
  await page.keyboard.press("Shift+Control+End");
  await page.keyboard.press("Control+C");
  let last = "";
  const end = Date.now() + TIMEOUT_MS;
  while ((last = await browserClipboard()) !== "pasted from the browser and back\n42" && Date.now() < end) {
    await page.waitForTimeout(100);
  }
  check(last === "pasted from the browser and back\n42", `Ctrl+V in VS Code pastes the result of the calculator: ${JSON.stringify(last)}`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "clipboard.png") });
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  if (SHOTS) await browser?.contexts()[0]?.pages()[0]?.screenshot({ path: join(SHOTS, "clipboard-fail.png") });
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
