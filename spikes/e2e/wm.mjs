// wm.mjs - end-to-end test of the window manager in Chromium
//
// Usage: npm --prefix spikes/e2e run wm
//
// The script starts farglassd in dev_user mode and the agent, and opens the
// shell in Chromium. It drags a window and measures the frames during the drag,
// snaps a window, changes a size, uses Alt+Tab, and reloads the page.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, launch, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
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

/** Returns the rectangle of the window element with the id `id`. */
async function box(page, id) {
  return page.locator(`[data-window="${id}"]`).boundingBox();
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
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 } });
  const cspViolations = [];
  context.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) cspViolations.push(message.text());
  });
  const page = await context.newPage();
  const scripts = [];
  page.on("request", (request) => {
    if (request.resourceType() === "script") scripts.push(new URL(request.url()).pathname);
  });
  await page.goto(url);
  const first = page.locator(".wm-window").first();
  await first.waitFor({ timeout: TIMEOUT_MS });
  const id = await first.getAttribute("data-window");

  // 1. A drag at the refresh rate of the screen.
  await page.evaluate((windowId) => {
    const element = document.querySelector(`[data-window="${windowId}"]`);
    const record = { frames: [], transforms: [], longTasks: [] };
    window.__wm = record;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) record.longTasks.push(entry.duration);
    }).observe({ type: "longtask", buffered: false });
    const sample = (time) => {
      record.frames.push(time);
      record.transforms.push(element.style.transform);
      if (record.frames.length < 2000 && !record.stop) requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  }, id);
  const before = await box(page, id);
  const title = page.locator(`[data-window="${id}"] .wm-title`);
  const grip = await title.boundingBox();
  await page.mouse.move(grip.x + 20, grip.y + 10);
  await page.mouse.down();
  for (let i = 1; i <= 90; i++) {
    await page.mouse.move(grip.x + 20 + i * 4, grip.y + 10 + i * 2);
    await page.waitForTimeout(16);
  }
  await page.mouse.up();
  const record = await page.evaluate(() => {
    window.__wm.stop = true;
    return window.__wm;
  });
  const intervals = record.frames.slice(1).map((time, i) => time - record.frames[i]).sort((a, b) => a - b);
  const p95 = intervals[Math.floor(intervals.length * 0.95)];
  const changes = record.transforms.filter((value, i) => i > 0 && value !== record.transforms[i - 1]).length;
  const after = await box(page, id);
  console.log(
    `INFO: ${record.frames.length} frames, frame interval median ${intervals[Math.floor(intervals.length / 2)].toFixed(1)} ms, ` +
      `p95 ${p95.toFixed(1)} ms, ${changes} transform changes, long tasks ${JSON.stringify(record.longTasks)}`,
  );
  check(Math.abs(after.x - before.x - 360) <= 2 && Math.abs(after.y - before.y - 180) <= 2, "the window follows the pointer");
  check(p95 < 25, `the frame interval stays at the refresh rate during the drag (p95 ${p95.toFixed(1)} ms)`);
  check(record.longTasks.every((duration) => duration < 50), "no long task during the drag");
  check(changes >= 30, `the transform changes in each frame of the drag (${changes} changes)`);

  // 2. Snap to the left half.
  const left = await title.boundingBox();
  await page.mouse.move(left.x + 20, left.y + 10);
  await page.mouse.down();
  await page.mouse.move(200, 400, { steps: 10 });
  await page.mouse.move(1, 400, { steps: 10 });
  await page.locator(".wm-snap-preview").waitFor({ timeout: TIMEOUT_MS });
  await page.mouse.up();
  const surface = await page.locator(".wm-surface").boundingBox();
  const snapped = await box(page, id);
  check(
    (await page.locator(`[data-window="${id}"]`).getAttribute("data-mode")) === "left" && Math.abs(snapped.width - surface.width / 2) <= 2,
    "a drag to the left edge snaps the window to the left half",
  );

  // 3. A second window, a size change, and Alt+Tab.
  await launch(page, "Echo");
  await page.locator(".wm-window").nth(1).waitFor({ timeout: TIMEOUT_MS });
  const second = await page.locator(".wm-window.wm-focused").getAttribute("data-window");
  const corner = await page.locator(`[data-window="${second}"] .wm-edge-se`).boundingBox();
  const small = await box(page, second);
  await page.mouse.move(corner.x + 6, corner.y + 6);
  await page.mouse.down();
  await page.mouse.move(corner.x + 106, corner.y + 56, { steps: 8 });
  await page.mouse.up();
  const large = await box(page, second);
  check(Math.abs(large.width - small.width - 100) <= 2 && Math.abs(large.height - small.height - 50) <= 2, "a drag of the corner changes the size");

  await page.keyboard.down("Alt");
  await page.keyboard.press("Tab");
  await page.locator(".wm-switcher").waitFor({ timeout: TIMEOUT_MS });
  await page.keyboard.up("Alt");
  check((await page.locator(".wm-window.wm-focused").getAttribute("data-window")) === id, "Alt+Tab gives the focus to the last used window");

  // The app in a window keeps its channel through the moves, the snap, and the focus changes.
  const firstWindow = page.locator(`[data-window="${id}"]`);
  await firstWindow.getByRole("status").filter({ hasText: "Echo channel open" }).waitFor({ timeout: TIMEOUT_MS });
  await firstWindow.getByLabel("Message").fill("hello from a moved window");
  await firstWindow.getByRole("button", { name: "Send" }).click();
  await firstWindow.locator('.echo-log li[data-kind="echo"]').filter({ hasText: "hello from a moved window" }).waitFor({ timeout: TIMEOUT_MS });
  check((await firstWindow.getByText(/^Channel \d+ is open\.$/).count()) === 1, "the app in a window keeps its channel through moves and snaps");

  // 5. The panel: the launcher and the session menu are popovers at their buttons.
  const appsButton = await page.getByRole("button", { name: "Apps" }).boundingBox();
  await page.getByRole("button", { name: "Apps" }).click();
  const launcher = page.locator("#launcher");
  await launcher.getByRole("combobox", { name: "Search apps" }).waitFor({ timeout: TIMEOUT_MS });
  check(await launcher.evaluate((el) => el.matches(":popover-open")), "the launcher is a popover in the top layer");
  const launcherBox = await launcher.boundingBox();
  check(
    launcherBox.y >= appsButton.y + appsButton.height && launcherBox.y <= appsButton.y + appsButton.height + 12 && Math.abs(launcherBox.x - appsButton.x) <= 2,
    "CSS anchor positioning puts the launcher under the Apps button",
  );
  check(await launcher.getByRole("combobox").evaluate((el) => el === document.activeElement), "the search field has the keyboard focus");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "launcher.png") });
  const aboutChunk = () => scripts.some((path) => /\/assets\/About-[\w-]+\.js$/.test(path));
  check(!aboutChunk() && scripts.some((path) => /\/assets\/Echo-[\w-]+\.js$/.test(path)), "the module of an app loads only when its first window opens");
  await page.keyboard.type("abou");
  await page.keyboard.press("Enter");
  await page.locator('.wm-window[aria-label="About farglass"]').getByRole("heading", { name: "farglass" }).waitFor({ timeout: TIMEOUT_MS });
  check(aboutChunk(), "the launcher loads the module of the About app and shows its window");
  check(!(await launcher.evaluate((el) => el.matches(":popover-open"))), "Enter opens the app and closes the launcher");

  await page.keyboard.press("Alt+F1");
  await launcher.getByRole("combobox").waitFor({ timeout: TIMEOUT_MS });
  await page.keyboard.press("Escape");
  await page.waitForTimeout(100);
  check(!(await launcher.evaluate((el) => el.matches(":popover-open"))), "Alt+F1 opens the launcher and Escape closes it");
  await page.keyboard.down("Meta");
  await page.keyboard.up("Meta");
  await launcher.getByRole("combobox").waitFor({ timeout: TIMEOUT_MS });
  await page.mouse.click(900, 500);
  await page.waitForTimeout(100);
  check(!(await launcher.evaluate((el) => el.matches(":popover-open"))), "a tap of Super opens the launcher and a click outside closes it");

  const session = page.locator(".panel-session");
  const sessionBox = await session.boundingBox();
  await session.click();
  const menu = page.locator("#session-menu");
  await menu.getByRole("menuitem", { name: "Log out" }).waitFor({ timeout: TIMEOUT_MS });
  const menuBox = await menu.boundingBox();
  check(
    menuBox.y >= sessionBox.y + sessionBox.height && Math.abs(menuBox.x + menuBox.width - (sessionBox.x + sessionBox.width)) <= 2,
    "the session menu is under the user button, at its right edge",
  );
  check((await menu.getByRole("menuitem", { name: "Lock" }).getAttribute("aria-disabled")) === "true", "the session menu has Lock, not available yet, and Log out");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "session.png") });
  await page.keyboard.press("Escape");

  if (SHOTS) await page.screenshot({ path: join(SHOTS, "desktop.png") });

  // 4. A reload gives the same desktop back from the state store of the agent.
  await page.waitForTimeout(400);
  await page.reload();
  await page.locator(`[data-window="${second}"]`).waitFor({ timeout: TIMEOUT_MS });
  const restored = await box(page, second);
  check(
    (await page.locator(`[data-window="${id}"]`).getAttribute("data-mode")) === "left" && Math.abs(restored.width - large.width) <= 2,
    "a reload gives back the windows, the snap, and the size",
  );
  check(cspViolations.length === 0, "no Content Security Policy violation");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
