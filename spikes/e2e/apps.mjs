// apps.mjs - end-to-end test of two Wayland apps side by side in Chromium
//
// Usage: npm --prefix spikes/e2e run apps
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket in a temporary directory. It starts gnome-calculator and VS Code on
// that socket, moves each window by the own title bar of the app, types in the
// two windows, and switches the focus between them. Each key must go to the
// window where it was typed: the title of VS Code shows the first line of its
// new file, and the calculator must not change while VS Code has the focus.
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
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1600, height: 1000 } })).newPage();
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

  // 1. Each window moves by the own title bar of the app.
  const drag = async (window, x, y, dx, dy) => {
    const before = await window.boundingBox();
    const content = await window.locator(".linux-app").boundingBox();
    await page.mouse.move(content.x + x, content.y + y);
    await page.mouse.down();
    await page.waitForTimeout(300);
    await page.mouse.move(content.x + x + dx, content.y + y + dy, { steps: 12 });
    await page.waitForTimeout(150);
    await page.mouse.up();
    await page.waitForTimeout(500);
    const after = await window.boundingBox();
    return [Math.round(after.x - before.x), Math.round(after.y - before.y)];
  };
  const codeBox = await code.locator(".linux-app").boundingBox();
  // A free part of the title bar of VS Code, right of the search box. The move takes VS Code away from the calculator.
  const movedCode = await drag(code, codeBox.width - 220, 17, 500, 30);
  check(movedCode[0] === 500 && movedCode[1] === 30, `VS Code moves by its own title bar (${movedCode.join(", ")})`);
  // The header bar of the calculator, between Undo and the mode button.
  const movedCalculator = await drag(calculator, 120, 20, 30, 20);
  check(movedCalculator[0] === 30 && movedCalculator[1] === 20, `the calculator moves by its own header bar (${movedCalculator.join(", ")})`);

  // 2. Typing in each window, with switches of the focus.
  const title = async () => (await code.getAttribute("aria-label")) ?? "";
  const calcCanvas = calculator.locator("canvas").first();
  // The display and the entry field of the calculator: they change when keys arrive. The header bar
  // changes too when the window loses the focus, thus it is not in the picture.
  const calcShot = async () => {
    const box = await calcCanvas.boundingBox();
    // The text cursor blinks after the result "15": the first 50 pixels stay out. Stray text would be longer.
    return (await page.screenshot({ clip: { x: box.x + 111, y: box.y + 300, width: 310, height: 115 } })).toString("base64");
  };
  const clickIn = async (window, x, y) => {
    const box = await window.locator(".linux-app").boundingBox();
    await page.mouse.click(box.x + x, box.y + y);
    await page.waitForTimeout(300);
  };
  await clickIn(code, 400, 300);
  await page.keyboard.press("Control+N");
  await page.waitForTimeout(1500);
  await page.keyboard.type("hello", { delay: 50 });
  await until(page, async () => (await title()).startsWith("● hello"), TIMEOUT_MS, "VS Code gets hello");
  check(true, `keys go to VS Code: ${await title()}`);

  const calcBefore = await calcShot();
  // The display of the calculator: a click gives it the focus.
  await clickIn(calculator, 150, 320);
  await page.keyboard.type("12", { delay: 50 });
  await page.keyboard.press("Shift+Equal");
  await page.keyboard.type("3", { delay: 50 });
  await page.keyboard.press("Enter");
  await until(page, async () => (await calcShot()) !== calcBefore, TIMEOUT_MS, "the calculator changes");
  check((await title()).startsWith("● hello •"), `keys go to the calculator, and VS Code keeps its text: ${await title()}`);

  await clickIn(code, 400, 300);
  await page.waitForTimeout(300);
  const calcAfter = await calcShot();
  await page.keyboard.press("End");
  await page.keyboard.type(" farglass", { delay: 50 });
  await until(page, async () => (await title()).startsWith("● hello farglass"), TIMEOUT_MS, "VS Code gets farglass");
  await page.waitForTimeout(500);
  const calcLast = await calcShot();
  if (SHOTS && calcLast !== calcAfter) {
    writeFileSync(join(SHOTS, "apps-calc-before.png"), Buffer.from(calcAfter, "base64"));
    writeFileSync(join(SHOTS, "apps-calc-after.png"), Buffer.from(calcLast, "base64"));
  }
  check(calcLast === calcAfter, `after a switch of the focus, keys go to VS Code only: ${await title()}`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "apps.png") });
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
