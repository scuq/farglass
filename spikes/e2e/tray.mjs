// tray.mjs - end-to-end test of the system tray in Chromium
//
// Usage: npm --prefix spikes/e2e run tray
//
// The script starts farglassd in dev_user mode and the agent. The agent serves
// the tray under a test name on the session bus, because the desktop of the
// host can own org.kde.StatusNotifierWatcher. The example program tray-item
// registers an item with a menu. The script checks the icon in the panel, a
// click, the menu with its check mark and its submenu, and that the item goes
// when the program stops.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { build, buildAll, root, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const NAME = `org.farglass.e2e.StatusNotifierWatcher${process.pid}`;
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

/** Waits until the output of `program` contains `text`. */
async function printed(program, text) {
  const end = Date.now() + TIMEOUT_MS;
  while (!program.out.includes(text)) {
    if (Date.now() > end) throw new Error(`the item did not print "${text}" in:\n${program.out}`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

try {
  buildAll();
  build("cargo", ["build", "-p", "farglass-session", "--example", "tray-item"]);
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
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket, FARGLASS_TRAY_NAME: NAME }, /serving the system tray/);
  children.push(agent.child);
  await agent.match;

  // 1. An item that registers before the shell connects waits in the agent.
  const item = { out: "", exited: false };
  const child = spawn("target/debug/examples/tray-item", [NAME], { cwd: root, stdio: ["ignore", "pipe", "inherit"] });
  child.stdout.on("data", (chunk) => {
    item.out += chunk;
  });
  child.on("exit", () => {
    item.exited = true;
  });
  children.push(child);
  await printed(item, "registered");

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  await page.goto(url);
  const button = page.locator('.panel-tray-item[aria-label="E2E item"]');
  await button.waitFor({ timeout: TIMEOUT_MS });
  check(true, "the item from before the shell connected shows in the panel");
  const icon = button.locator("img");
  check((await icon.getAttribute("src")).startsWith("data:image/png;base64,"), "the icon is a PNG file from the pixels of the item");
  check(await icon.evaluate((img) => img.complete && img.naturalWidth === 2 && img.naturalHeight === 2), "the browser decodes the icon with its size of 2 x 2 pixels");
  const box = await button.boundingBox();
  const keyboard = await page.locator(".panel-keyboard").boundingBox();
  check(box.y < 40 && box.x + box.width <= keyboard.x, "the tray is in the panel, left of the keyboard layout");

  // 2. A click does the main action of the item.
  await button.click();
  await printed(item, "activate ");
  check(true, "a click sends Activate to the program");

  // 3. A right click opens the menu of the program.
  await button.click({ button: "right" });
  const menu = page.locator(".context-menu").first();
  const entry = (name) => menu.locator(".context-menu-item").filter({ has: page.locator(`.context-menu-label:text-is("${name}")`) });
  await entry("Show window").waitFor({ timeout: TIMEOUT_MS });
  check((await menu.locator(":scope > .context-menu-item > .context-menu-label").allTextContents()).join("|") === "Show window|Sync|More", "the menu has the entries of the program, with no hidden entry and no mnemonic mark");
  check((await entry("Sync").first().getAttribute("aria-checked")) === "true", "an entry with a check mark is checked");
  const menuBox = await menu.boundingBox();
  check(menuBox.y >= box.y + box.height - 1 && menuBox.x + menuBox.width <= 1280, "the menu opens below the icon and in the viewport");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "tray-menu.png") });

  // 4. The pointer on an entry with a submenu opens it, in the viewport.
  await entry("More").first().hover();
  const sub = menu.locator(".context-menu-sub");
  await sub.waitFor({ timeout: TIMEOUT_MS });
  const subBox = await sub.boundingBox();
  check(subBox.x >= 0 && subBox.x + subBox.width <= 1280, "the submenu is in the viewport");
  check((await sub.locator(".context-menu-item").filter({ hasText: "Lock" }).getAttribute("aria-disabled")) === "true", "a disabled entry is disabled");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "tray-submenu.png") });

  // 5. A click on an entry goes to the program. Quit stops it, and its item goes.
  await sub.locator(".context-menu-item").filter({ hasText: "Quit" }).click();
  await printed(item, "event 7 clicked");
  check(true, "a click on an entry of the submenu sends the event clicked");
  await button.waitFor({ state: "detached", timeout: TIMEOUT_MS });
  check((await page.locator(".panel-tray").count()) === 0, "the item goes when its program stops, and the empty tray does not show");

  // 6. The keyboard: Shift+F10 on a new item opens its menu, and the arrow keys and Enter select.
  const again = { out: "" };
  const second = spawn("target/debug/examples/tray-item", [NAME], { cwd: root, stdio: ["ignore", "pipe", "inherit"] });
  second.stdout.on("data", (chunk) => {
    again.out += chunk;
  });
  children.push(second);
  await button.waitFor({ timeout: TIMEOUT_MS });
  await button.focus();
  await page.keyboard.press("Shift+F10");
  await entry("Show window").waitFor({ timeout: TIMEOUT_MS });
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await printed(again, "event 2 clicked");
  check(true, "Shift+F10 opens the menu, and the arrow keys and Enter select an entry");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
