// popups.mjs - end-to-end test of the popups and the bubbles of a host browser
//
// Usage: npm --prefix spikes/e2e run popups
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket in a temporary directory, and a Chromium on that socket. Chromium
// draws its menus as xdg popups and its bubbles and the list of suggestions of
// the address bar as subsurfaces. The shell must show each of them: the main
// menu and a submenu past the edge of the window, the list of suggestions,
// and the panel of the extensions.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, raise, root, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let host;
let failed = false;

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

try {
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
  const shell = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1600, height: 900 } })).newPage();
  const errors = [];
  shell.on("pageerror", (error) => errors.push(error.message));
  await shell.goto(url);
  await shell.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  // An extension, thus Chromium shows its button of the extensions.
  const extension = join(root, "extension", "redirect");
  host = await chromium.launchPersistentContext(join(state, "profile"), {
    headless: false,
    viewport: null,
    args: ["--ozone-platform=wayland", "--window-size=900,600", `--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
    env: { ...process.env, WAYLAND_DISPLAY: wayland, DISPLAY: "" },
  });
  const window = shell.locator(".wm-window").filter({ has: shell.locator(".linux-app") }).first();
  await window.waitFor({ timeout: 60_000 });
  await shell.waitForTimeout(2000);
  await raise(shell, window);
  const box = await window.locator(".linux-app").boundingBox();
  const canvases = () => window.locator("canvas[data-surface]").count();
  const shot = async (name) => SHOTS && writeFileSync(join(SHOTS, name), await shell.screenshot());

  // 1. The list of suggestions of the address bar: a subsurface.
  await shell.mouse.click(box.x + 300, box.y + 56);
  await shell.keyboard.type("wiki", { delay: 60 });
  await shell.waitForFunction(() => document.querySelectorAll(".linux-app canvas[data-surface]").length >= 2, null, { timeout: TIMEOUT_MS });
  await shot("popups-omnibox.png");
  check(true, "the list of suggestions of the address bar shows");
  await shell.keyboard.press("Escape");
  await shell.keyboard.press("Escape");
  await shell.waitForTimeout(1000);

  // 2. The panel of the extensions: a subsurface.
  const before = await canvases();
  await shell.mouse.click(box.x + box.width - 113, box.y + 62);
  await shell.waitForFunction((n) => document.querySelectorAll(".linux-app canvas[data-surface]").length > n, before, { timeout: TIMEOUT_MS }).catch(() => {});
  await shot("popups-extensions.png");
  check((await canvases()) > before, "the panel of the extensions shows");
  await shell.keyboard.press("Escape");
  await shell.waitForTimeout(1000);

  // 3. The main menu and a submenu: popups that reach past the window.
  await shell.mouse.click(box.x + box.width - 20, box.y + 56);
  // The popups of the window: each canvas after the canvas of the window, with its box.
  const popupBoxes = () => window.locator("canvas[data-surface]").evaluateAll((all) => all.slice(1).map((c) => { const r = c.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; }));
  const tallest = (boxes) => boxes.reduce((a, b) => (b.height > (a?.height ?? 0) ? b : a), null);
  const opened = (await popupBoxes()).length;
  await shell.waitForFunction((n) => document.querySelectorAll(".linux-app canvas[data-surface]").length - 1 > n, opened - 1, { timeout: TIMEOUT_MS });
  await shell.waitForTimeout(800);
  const menu = tallest(await popupBoxes());
  check(menu.y + menu.height > box.y + box.height, `the menu reaches below the window (${Math.round(menu.y + menu.height)} > ${Math.round(box.y + box.height)})`);
  const count = await canvases();
  for (let y = menu.y + 120; y < menu.y + menu.height - 20 && (await canvases()) === count; y += 32) {
    await shell.mouse.move(menu.x + menu.width / 2, y, { steps: 3 });
    await shell.waitForTimeout(500);
  }
  const sub = (await popupBoxes()).find((b) => b.x > menu.x + menu.width / 2 && b !== menu) ?? (await popupBoxes()).at(-1);
  await shot("popups-submenu.png");
  check((await canvases()) > count && sub.x + sub.width > box.x + box.width, `a submenu shows past the right edge of the window (${Math.round(sub.x + sub.width)} > ${Math.round(box.x + box.width)})`);
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await host?.close().catch(() => {});
  await browser?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
