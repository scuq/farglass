// tabs.mjs - end-to-end test of the tabs of the terminal (F108) and of Settings > Terminal (F107) in Chromium
//
// Usage: npm --prefix spikes/e2e run tabs
//
// The script builds the programs, starts farglassd in dev_user mode and the agent,
// and opens a terminal window. It opens tabs with the + button and with Alt+Shift+Enter,
// switches them with Alt+PageDown, reloads the page, and closes a tab. It splits panes
// with keys, drags a divider, and closes panes (F109). Then it changes the
// color profile and the font in Settings > Terminal.
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
  if (!condition) throw new Error(text);
  console.log(`PASS: ${text}`);
}

/** The terminal of the window `id` that has the focus: of the active tab, the focused pane. */
const visible = (page, id) => page.locator(`[data-window="${id}"] .term-cell[data-focused]:not([hidden]) .term-app`);
/** The number of panes that show in the window `id`. */
const shown = (page, id) => page.locator(`[data-window="${id}"] .term-cell:not([hidden])`).count();

/** Waits until the visible terminal of the window `id` shows `pattern`. */
async function until(page, id, pattern) {
  const end = Date.now() + TIMEOUT_MS;
  for (;;) {
    const current = await visible(page, id).evaluate((el) => el.farglassTerminal.text());
    if (pattern.test(current)) return current;
    if (Date.now() > end) throw new Error(`The terminal does not show ${pattern}:\n${current.split("\n").slice(-10).join("\n")}`);
    await page.waitForTimeout(50);
  }
}

/** Types a command in the visible terminal of the window `id`. */
async function run(page, id, command) {
  await visible(page, id).click();
  await page.keyboard.type(command);
  await page.keyboard.press("Enter");
}

const tabNames = (page, id) => page.locator(`[data-window="${id}"] .term-tab`).evaluateAll((els) => els.map((el) => el.getAttribute("aria-selected")));

try {
  const bin = buildAll(process.env.FARGLASS_RELEASE === "1");
  const socket = join(state, "agent.sock");
  const daemon = start(
    "farglassd",
    `${bin}/farglassd`,
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start("farglass-session", `${bin}/farglass-session`, [], { FARGLASS_AGENT_SOCKET: socket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  page.on("pageerror", (error) => console.log(`INFO: page error ${error.message}`));
  await page.goto(url);
  await page.locator(".wm-window").first().waitFor({ timeout: TIMEOUT_MS });

  // 1. A terminal window has one tab and no tab bar.
  await launch(page, "Terminal");
  const window = page.locator(".wm-window").filter({ has: page.locator('.term-app[data-status="running"]') }).first();
  await window.waitFor({ timeout: TIMEOUT_MS });
  const id = await window.getAttribute("data-window");
  check((await page.locator(`[data-window="${id}"] .term-tabs`).count()) === 0, "a window with one tab shows no tab bar");
  await run(page, id, "echo first-$((1+1))");
  await until(page, id, /first-2/);

  // 2. Alt+Shift+Enter opens a second tab with its own shell.
  await page.keyboard.press("Alt+Shift+Enter");
  await page.locator(`[data-window="${id}"] .term-tab`).nth(1).waitFor({ timeout: TIMEOUT_MS });
  await page.locator(`[data-window="${id}"] .term-app[data-status="running"]:not([hidden])`).waitFor({ timeout: TIMEOUT_MS });
  check((await tabNames(page, id)).join() === "false,true", "Alt+Shift+Enter opens a second tab and shows it");
  await run(page, id, "echo second-$((2+2))");
  const second = await until(page, id, /second-4/);
  check(!second.includes("first-2"), "the second tab has its own shell");

  // 3. The + button opens a third tab.
  await page.locator(`[data-window="${id}"] .term-tab-new`).click();
  await page.locator(`[data-window="${id}"] .term-tab`).nth(2).waitFor({ timeout: TIMEOUT_MS });
  check((await tabNames(page, id)).join() === "false,false,true", "the + button opens a third tab");

  // 4. Alt+PageDown goes to the next tab, with wrap-around.
  await page.locator(`[data-window="${id}"] .term-app[data-status="running"]:not([hidden])`).waitFor({ timeout: TIMEOUT_MS });
  await visible(page, id).click();
  await page.keyboard.press("Alt+PageDown");
  check((await tabNames(page, id)).join() === "true,false,false", "Alt+PageDown goes from the last tab to the first tab");
  await until(page, id, /first-2/);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "tabs.png") });

  // 5. A reload keeps the tabs and their shells.
  await page.reload();
  await page.locator(`[data-window="${id}"] .term-tab`).nth(2).waitFor({ timeout: TIMEOUT_MS });
  await page.locator(`[data-window="${id}"] .term-tab`).nth(1).click();
  await until(page, id, /second-4/);
  check(true, "a reload keeps the three tabs and the scrollback of each shell");

  // 6. exit closes the tab. Its shell stops.
  await run(page, id, "exit");
  await page.waitForFunction((win) => document.querySelectorAll(`[data-window="${win}"] .term-tab`).length === 2, id, { timeout: TIMEOUT_MS });
  check(true, "exit in a tab closes the tab");
  await page.locator(`[data-window="${id}"] .term-tab-close`).last().click();
  await page.waitForFunction((win) => document.querySelectorAll(`[data-window="${win}"] .term-tabs`).length === 0, id, { timeout: TIMEOUT_MS });
  check(true, "the close button closes a tab, and with one tab the tab bar goes");
  await until(page, id, /first-2/);

  // 7. Alt+Shift+\\ splits the pane. The new pane on the right has its own shell and the focus.
  await visible(page, id).click();
  await page.keyboard.press("Alt+Shift+Backslash");
  await page.locator(`[data-window="${id}"] .term-cell[data-focused] .term-app[data-status="running"]`).waitFor({ timeout: TIMEOUT_MS });
  check((await shown(page, id)) === 2, "Alt+Shift+\\ splits the pane in two");
  await page.keyboard.type("echo right-$((3+3))");
  await page.keyboard.press("Enter");
  const right = await until(page, id, /right-6/);
  check(!right.includes("first-2"), "the new pane has its own shell and gets the focus");
  const cells = await page.locator(`[data-window="${id}"] .term-cell:not([hidden])`).evaluateAll((els) => els.map((el) => el.getBoundingClientRect().x));
  check(cells[1] > cells[0] + 100, "the new pane is right of the first pane");

  // 8. Alt+Shift+- splits down, Alt+Shift+Left goes back to the first pane.
  await page.keyboard.press("Alt+Shift+Minus");
  await page.waitForFunction((win) => document.querySelectorAll(`[data-window="${win}"] .term-cell:not([hidden]) .term-app[data-status="running"]`).length === 3, id, { timeout: TIMEOUT_MS });
  check(true, "Alt+Shift+- splits the right pane down");
  await page.keyboard.press("Alt+Shift+ArrowLeft");
  await until(page, id, /first-2/);
  check(true, "Alt+Shift+Left gives the focus to the pane on the left");

  // 9. A drag of the divider changes the sizes. A reload keeps the panes and the sizes.
  const divider = page.locator(`[data-window="${id}"] .term-divider[data-dir="row"]`);
  const bar = await divider.boundingBox();
  // A quarter down: the middle of the divider is the end of the divider of the right panes.
  await page.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 4);
  await page.mouse.down();
  await page.mouse.move(bar.x - 120, bar.y + bar.height / 4, { steps: 5 });
  await page.mouse.up();
  const widths = () => page.locator(`[data-window="${id}"] .term-cell:not([hidden])`).evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().width)));
  const dragged = await widths();
  check(dragged[0] < dragged[1] - 150, `a drag of the divider makes the left pane smaller (${dragged.join(", ")} px)`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "splits.png") });
  await page.reload();
  await page.waitForFunction((win) => document.querySelectorAll(`[data-window="${win}"] .term-cell:not([hidden]) .term-app[data-status="running"]`).length === 3, id, { timeout: TIMEOUT_MS });
  const reloaded = await widths();
  check(Math.abs(reloaded[0] - dragged[0]) <= 2, "a reload keeps the three panes and their sizes");

  // 10. exit closes a pane. Its neighbor gets the space.
  await page.locator(`[data-window="${id}"] .term-cell:not([hidden]) .term-app`).nth(2).click();
  await page.keyboard.type("exit");
  await page.keyboard.press("Enter");
  await page.waitForFunction((win) => document.querySelectorAll(`[data-window="${win}"] .term-cell:not([hidden])`).length === 2, id, { timeout: TIMEOUT_MS });
  check(true, "exit in a pane closes the pane");
  await page.locator(`[data-window="${id}"] .term-cell:not([hidden]) .term-app`).nth(1).click();
  await page.keyboard.type("exit");
  await page.keyboard.press("Enter");
  await page.waitForFunction((win) => document.querySelectorAll(`[data-window="${win}"] .term-cell:not([hidden])`).length === 1, id, { timeout: TIMEOUT_MS });
  await until(page, id, /first-2/);
  check(true, "with one pane left, the first shell fills the window again");

  // 11. Settings > Terminal changes the colors and the font of the open terminal at once.
  const term = visible(page, id);
  const before = await term.evaluate((el) => getComputedStyle(el).getPropertyValue("--term-bg"));
  await launch(page, "Settings");
  // The first login opens Settings too: use the new window, which has the focus.
  const settings = page.locator('.wm-window.wm-focused[aria-label^="Settings"]');
  await settings.locator('[data-page="terminal"]').click();
  await settings.getByLabel("Nord").check();
  await page.waitForFunction((win) => getComputedStyle(document.querySelector(`[data-window="${win}"] .term-app`)).getPropertyValue("--term-bg").trim() === "#2e3440", id, { timeout: TIMEOUT_MS });
  check(before.trim() !== "#2e3440", "the color profile Nord applies at once to the open terminal");
  const cols = () => term.evaluate((el) => el.farglassTerminal.cols());
  const before14 = await cols();
  await settings.getByLabel("Size", { exact: true }).selectOption("20");
  await page.waitForFunction(([win, old]) => document.querySelector(`[data-window="${win}"] .term-app:not([hidden])`).farglassTerminal.cols() < old * 0.8, [id, before14], { timeout: TIMEOUT_MS });
  console.log(`INFO: ${before14} columns at 14 px, ${await cols()} columns at 20 px`);
  check(true, "the font size 20 px applies at once to the open terminal");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "terminal-settings.png") });
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
  if (SHOTS) await browser?.contexts()[0]?.pages()[0]?.screenshot({ path: join(SHOTS, "fail.png") }).catch(() => {});
} finally {
  await browser?.close();
  for (const child of children) child.kill();
  rmSync(state, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
