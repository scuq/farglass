// term.mjs - end-to-end test of the terminal app in Chromium
//
// Usage: npm --prefix spikes/e2e run term
//
// The script builds the programs with optimizations, starts farglassd in
// dev_user mode and the agent, and opens terminals in the shell. It measures
// the delay from a key press to its echo with the Performance API, and tests
// the clipboard, a size change, a reload, a closed tab, and the close of a window.
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

/** Returns the text of the terminal in the window `id`. */
function text(page, id) {
  return page.locator(`[data-window="${id}"] .term-app`).evaluate((el) => el.farglassTerminal.text());
}

/** Waits until the terminal in the window `id` shows `pattern`. Returns the text. */
async function until(page, id, pattern, timeout = TIMEOUT_MS) {
  const end = Date.now() + timeout;
  for (;;) {
    const current = await text(page, id);
    if (pattern.test(current)) return current;
    if (Date.now() > end) throw new Error(`The terminal does not show ${pattern}:\n${current.split("\n").slice(-15).join("\n")}`);
    await page.waitForTimeout(50);
  }
}

/** Types a command in the terminal of the window `id`. */
async function run(page, id, command) {
  await page.locator(`[data-window="${id}"] .term-app`).click();
  await page.keyboard.type(command);
  await page.keyboard.press("Enter");
}

/** Opens a terminal window and returns its id when its shell runs. */
async function openTerminal(page) {
  const before = await page.locator(".wm-window").evaluateAll((els) => els.map((el) => el.dataset.window));
  await launch(page, "Terminal");
  const window = page.locator(".wm-window").filter({ has: page.locator('.term-app[data-status="running"]') });
  for (;;) {
    const ids = await window.evaluateAll((els) => els.map((el) => el.dataset.window));
    const id = ids.find((each) => !before.includes(each));
    if (id) return id;
    await page.waitForTimeout(50);
  }
}

try {
  const bin = buildAll(true);
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
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: new URL(url).origin });
  const cspViolations = [];
  context.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) cspViolations.push(message.text());
  });
  let page = await context.newPage();
  await page.goto(url);
  await page.locator(".wm-window").first().waitFor({ timeout: TIMEOUT_MS });

  // 1. A shell in a terminal window.
  const id = await openTerminal(page);
  const renderer = await page.locator(`[data-window="${id}"] .term-app`).getAttribute("data-renderer");
  console.log(`INFO: the terminal uses the ${renderer} renderer`);
  await run(page, id, "MARK=kept-$((6*7)); echo shell-$MARK; echo pid=$$");
  const first = await until(page, id, /shell-kept-42/);
  const pid = Number(/pid=(\d+)/.exec(first)[1]);
  check(pid > 0, `the terminal runs a shell (pid ${pid})`);

  // 2. The delay from a key press to its echo.
  // Warm up the terminal and the JIT of the browser, then measure 60 key presses.
  for (const key of "warmupkeys") {
    await page.keyboard.press(key);
    await page.waitForTimeout(25);
  }
  await page.waitForTimeout(200);
  await page.evaluate(() => performance.clearMeasures("farglass:terminal-echo"));
  // 50 ms between keys is 20 keys each second, fast typing. Headless Chromium draws
  // WebGL in software: a shorter gap measures its frames, not the path to the host.
  for (const key of "abcdefghijklmnopqrstuvwxyz".repeat(3).slice(0, 60)) {
    await page.keyboard.press(key);
    await page.waitForTimeout(50);
  }
  await page.keyboard.press("Control+U");
  const delays = (await page.evaluate(() => performance.getEntriesByName("farglass:terminal-echo").map((m) => m.duration))).sort(
    (a, b) => a - b,
  );
  const p50 = delays[Math.floor(delays.length / 2)];
  const p95 = delays[Math.floor(delays.length * 0.95)];
  console.log(`INFO: ${delays.length} echoes, median ${p50.toFixed(2)} ms, p95 ${p95.toFixed(2)} ms, max ${delays.at(-1).toFixed(2)} ms`);
  check(delays.length >= 55, "the Performance API has a measurement for each key press");
  check(p95 < 5, `the echo of a key press comes in less than 5 ms on loopback (p95 ${p95.toFixed(2)} ms)`);

  // 3. The clipboard: OSC 52 from the host, and Ctrl+Shift+V.
  await run(page, id, "printf '\\033]52;c;%s\\a' \"$(printf from-osc52 | base64)\"");
  await page.waitForFunction(async () => (await navigator.clipboard.readText()) === "from-osc52", null, { timeout: TIMEOUT_MS });
  check(true, "a program on the host sets the clipboard of the browser with OSC 52, right after a key");
  // With no key for 4 s, the terminal asks first (F055).
  await page.evaluate(() => navigator.clipboard.writeText("before"));
  await run(page, id, "sleep 4; printf '\\033]52;c;%s\\a' \"$(printf late-osc52 | base64)\"");
  const ask = page.locator(`[data-window="${id}"] .term-copy`);
  await ask.waitFor({ timeout: TIMEOUT_MS });
  check((await page.evaluate(() => navigator.clipboard.readText())) === "before", "a late OSC 52 write does not change the clipboard by itself");
  await ask.getByRole("button", { name: "Ignore" }).click();
  check((await page.evaluate(() => navigator.clipboard.readText())) === "before" && (await ask.count()) === 0, "Ignore drops the write");
  await run(page, id, "sleep 4; printf '\\033]52;c;%s\\a' \"$(printf late-osc52 | base64)\"");
  await ask.waitFor({ timeout: TIMEOUT_MS });
  await ask.getByRole("button", { name: "Copy" }).click();
  await page.waitForFunction(async () => (await navigator.clipboard.readText()) === "late-osc52", null, { timeout: TIMEOUT_MS });
  check(true, "Copy in the question writes the clipboard");
  await page.evaluate(() => navigator.clipboard.writeText("echo pasted-$((6*7))"));
  await page.locator(`[data-window="${id}"] .term-app`).click();
  await page.keyboard.press("Control+Shift+V");
  await page.keyboard.press("Enter");
  await until(page, id, /pasted-42/);
  check(true, "Ctrl+Shift+V pastes the clipboard of the browser, and Enter right after it comes after the pasted text");

  // The ssh-agent of the session (F093): the shell has SSH_AUTH_SOCK, and ssh-add reaches an agent with no key.
  await run(page, id, 'ssh-add -l; echo "agent-status-$?-${SSH_AUTH_SOCK##*-}"');
  await until(page, id, /agent-status-1-agent/);
  check(true, "the shell has the ssh-agent of the session, with no key");

  // 4. The size of the window gives the size of the terminal.
  await page.locator(`[data-window="${id}"]`).getByRole("button", { name: "Maximize" }).click();
  await page.waitForTimeout(300);
  await run(page, id, "stty size");
  const size = /\n(\d+) (\d+)\s*$/m.exec(await until(page, id, /\n\d+ \d+\s*$/m));
  check(Number(size[2]) > 120, `a larger window gives more columns (stty size: ${size[1]} ${size[2]})`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "terminal.png") });

  // 4b. The context menus of the shell.
  const menu = page.locator(".context-menu");
  const item = (label) => menu.getByRole("menuitem", { name: new RegExp(`^${label}`) });
  await page.locator(`[data-window="${id}"] .wm-title`).click({ button: "right" });
  await item("Restore").waitFor({ timeout: TIMEOUT_MS });
  check(await menu.evaluate((el) => el.matches(":popover-open")), "a right click on the title bar opens the menu of the shell");
  await item("Restore").click();
  await page.waitForFunction((windowId) => document.querySelector(`[data-window="${windowId}"]`).dataset.mode === "normal", id);
  check(!(await menu.evaluate((el) => el.matches(":popover-open"))), "an item runs its action and closes the menu");

  await page.evaluate(() => navigator.clipboard.writeText(""));
  await page.locator(`[data-window="${id}"] .term-app`).click({ button: "right" });
  await item("Select all").click();
  await page.locator(`[data-window="${id}"] .term-app`).click({ button: "right" });
  await item("Copy").click();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  check(copied.includes("shell-kept-42"), "the terminal menu selects all and copies the text to the clipboard");
  await page.evaluate(() => navigator.clipboard.writeText("echo menu-paste-$((7*6))"));
  await page.locator(`[data-window="${id}"] .term-app`).click({ button: "right" });
  await item("Paste").click();
  await page.keyboard.press("Enter");
  await until(page, id, /menu-paste-42/);
  check(true, "the terminal menu pastes the clipboard");

  await page.locator(`[data-window="${id}"] .term-app`).click();
  await page.keyboard.press("Shift+F10");
  await item("Copy").waitFor({ timeout: TIMEOUT_MS });
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "menu.png") });
  await page.keyboard.press("Escape");
  check(true, "Shift+F10 opens the menu of the focused terminal");

  // A text field gets the text menu of the shell, and the launcher stays open.
  await page.evaluate(() => navigator.clipboard.writeText("abo"));
  await page.getByRole("button", { name: "Apps" }).click();
  const search = page.locator("#launcher").getByRole("combobox", { name: "Search apps" });
  await search.waitFor({ timeout: TIMEOUT_MS });
  await search.click({ button: "right" });
  await item("Paste").waitFor({ timeout: TIMEOUT_MS });
  check((await menu.getByRole("menuitem").allTextContents()).join(",") === "Cut,Copy,Paste,Select all", "a text field gets the text menu of the shell");
  await item("Paste").click();
  // The paste reads the clipboard asynchronously.
  await page.waitForFunction(() => document.querySelector(".launcher-search")?.value === "abo", null, { timeout: TIMEOUT_MS });
  check(
    (await search.inputValue()) === "abo" && (await page.locator("#launcher").evaluate((el) => el.matches(":popover-open"))),
    "Paste from the text menu fills the search, and the launcher stays open",
  );
  await page.keyboard.press("Escape");

  const surface = await page.locator(".wm-surface").boundingBox();
  await page.mouse.click(surface.x + surface.width - 20, surface.y + surface.height - 20, { button: "right" });
  await item("Open About farglass").click();
  const about = page.locator('.wm-window[aria-label="About farglass"]');
  await about.waitFor({ timeout: TIMEOUT_MS });
  check(true, "the menu of the empty desktop opens an app");
  // About reads hostname1 on the system bus through the dbus channel of the agent.
  const osRelease = (await import("node:fs")).readFileSync("/etc/os-release", "utf8");
  const pretty = /^PRETTY_NAME="?([^"\n]*)"?$/m.exec(osRelease)?.[1];
  await about.locator('[data-field="os"]').filter({ hasText: pretty }).waitFor({ timeout: TIMEOUT_MS });
  check(true, `About shows the operating system of the host from D-Bus (${pretty})`);
  await about.getByRole("button", { name: "Close" }).click();

  // 5. A reload attaches to the same shell, with its scrollback.
  await page.waitForTimeout(400);
  await page.reload();
  await page.locator(`[data-window="${id}"] .term-app[data-status="running"]`).waitFor({ timeout: TIMEOUT_MS });
  await until(page, id, /shell-kept-42/);
  await run(page, id, "echo again-$MARK");
  await until(page, id, /again-kept-42/);
  check(true, "a reload attaches to the same shell, with its scrollback and its variables");

  // 6. A job continues when the tab closes.
  await run(page, id, "sleep 1; echo job-finished-$MARK");
  await page.close();
  await new Promise((resolve) => setTimeout(resolve, 2000));
  page = await context.newPage();
  await page.goto(url);
  await page.locator(`[data-window="${id}"] .term-app[data-status="running"]`).waitFor({ timeout: TIMEOUT_MS });
  await until(page, id, /job-finished-kept-42/);
  check(true, "a job continues with no tab, and its output is there after the next connection");

  // 7. More than one window, and the close of a window ends its shell.
  const second = await openTerminal(page);
  await run(page, second, "echo pid=$$");
  const secondPid = Number(/pid=(\d+)/.exec(await until(page, second, /pid=\d+/))[1]);
  check(secondPid !== pid, "a second terminal window has its own shell");
  await page.locator(`[data-window="${second}"]`).getByRole("button", { name: "Close" }).click();
  const end = Date.now() + TIMEOUT_MS;
  let alive = true;
  while (alive && Date.now() < end) {
    try {
      process.kill(secondPid, 0);
      await new Promise((resolve) => setTimeout(resolve, 100));
    } catch {
      alive = false;
    }
  }
  check(!alive, "closing a terminal window ends its shell");
  check(cspViolations.length === 0, `no Content Security Policy violation ${cspViolations.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
