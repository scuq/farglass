// surface.mjs - end-to-end test of a Wayland app window in Chromium
//
// Usage: npm --prefix spikes/e2e run surface
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket in a temporary directory. It starts gnome-calculator on that socket,
// and the shell must show its window with its pixels. A resize of the window
// must give a new buffer, and the close button must stop the app. The script
// prints the CPU time of the encoder for each frame.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, start, TIMEOUT_MS, raise } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const APP = "/usr/bin/gnome-calculator";
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

/** Returns the number of distinct colors in the screenshot of `locator`. A blank page decodes it: the shell has a strict CSP. */
async function colors(page, locator) {
  const png = await locator.screenshot();
  const blank = await page.context().newPage();
  const count = await blank.evaluate(async (base64) => {
    const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${base64}`)).blob());
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d");
    context.drawImage(bitmap, 0, 0);
    const data = context.getImageData(0, 0, bitmap.width, bitmap.height).data;
    const set = new Set();
    for (let i = 0; i < data.length; i += 4) set.add((data[i] << 16) | (data[i + 1] << 8) | data[i + 2]);
    return set.size;
  }, png.toString("base64"));
  await blank.close();
  return count;
}

try {
  if (!existsSync(APP)) {
    console.log("SKIP: no gnome-calculator");
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
  const agent = start(
    "farglass-session",
    "target/debug/farglass-session",
    [],
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, RUST_LOG: "info,farglass_session::surface_stream=trace" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  const app = spawn(APP, [], {
    env: { ...process.env, WAYLAND_DISPLAY: wayland, GDK_BACKEND: "wayland", DBUS_SESSION_BUS_ADDRESS: "disabled:", DISPLAY: "" },
    stdio: "ignore",
  });
  children.push(app);
  let exited = null;
  app.on("exit", (code) => (exited = code));

  // 1. The window shows with the title and the pixels of the app.
  const window = page.locator(".wm-window").filter({ has: page.locator(".linux-app") });
  await window.waitFor({ timeout: 30_000 });
  // The test started the app, not the user: the window opens behind Echo, with a mark in the panel (F041).
  const task = page.locator(".panel-tasks .wm-task").filter({ hasText: await window.getAttribute("aria-label") }).first();
  if ((await task.getAttribute("data-attention")) !== "true") {
    throw new Error("the window that the user did not ask for has no mark in the panel");
  }
  console.log("PASS: a window that the user did not ask for opens in the background with a mark in the panel");
  await raise(page, window);
  if ((await task.getAttribute("data-attention")) === "true") {
    throw new Error("the mark stays after the focus");
  }
  await page.waitForFunction(() => document.querySelector(".linux-app canvas")?.style.width, null, { timeout: TIMEOUT_MS });
  await page.waitForTimeout(500);
  check((await window.getAttribute("aria-label")) === "Calculator", "the window of the app shows with its title");
  const content = window.locator(".linux-app");
  const shown = await colors(page, content);
  check(shown > 20, `the window shows the pixels of the app (${shown} colors)`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "surface.png") });
  check(!(await page.locator("#launcher [role=option]").filter({ hasText: "Linux app" }).count()), "the launcher does not list the window app");

  // 2. Input: keys and clicks reach the app, and its cursor shows (F029).
  await window.locator(".wm-titlebar").click({ position: { x: 150, y: 10 } });
  const canvas = content.locator("canvas").first();
  const shot = async () => (await canvas.screenshot()).toString("base64");
  const initial = await shot();
  await page.keyboard.press("7");
  await page.keyboard.press("Shift+Equal");
  await page.keyboard.press("2");
  await page.keyboard.press("Enter");
  let typed = initial;
  for (let i = 0; i < 30 && typed === initial; i++) {
    await page.waitForTimeout(100);
    typed = await shot();
  }
  check(typed !== initial, "keys reach the app: 7 + 2 Enter changes its window");
  // The display of the calculator has a text cursor.
  const area = await canvas.boundingBox();
  // The canvas includes the shadow of GTK (61, 55): the entry field is at (161, 365) and the C button at (104, 445).
  // GTK sets the cursor on motion events: the pointer moves in steps, as a real pointer does.
  await page.mouse.move(area.x + 161, area.y + 365, { steps: 6 });
  await page.waitForFunction(() => document.querySelector(".linux-app")?.dataset.cursor === "text", null, { timeout: TIMEOUT_MS }).catch(() => {});
  const cursor = await content.getAttribute("data-cursor");
  check(cursor === "text", `the cursor of the app shows: ${cursor} over the entry field`);
  // A click on the C button clears the display.
  const clear = await shot();
  await page.mouse.click(area.x + 104, area.y + 445);
  let clicked = clear;
  for (let i = 0; i < 30 && clicked === clear; i++) {
    await page.waitForTimeout(100);
    clicked = await shot();
  }
  check(clicked !== clear, "a click on C reaches the app and clears the display");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "surface-input.png") });

  // 3. A new size of the window (maximize) gives a buffer of the new size.
  const before = await content.evaluate((el) => el.querySelector("canvas").style.width);
  await window.getByRole("button", { name: "Maximize" }).click();
  await page.waitForFunction((old) => document.querySelector(".linux-app canvas")?.style.width !== old, before, { timeout: TIMEOUT_MS });
  check(true, `a resize gives a new buffer (${before} to ${await content.evaluate((el) => el.querySelector("canvas").style.width)})`);

  // 4. The encoder time of each frame on the host.
  const times = [...agent.log().matchAll(/micros: (\d+)/g)].map((match) => Number(match[1])).sort((a, b) => a - b);
  const bytes = [...agent.log().matchAll(/bytes: (\d+)/g)].map((match) => Number(match[1]));
  const cached = [...agent.log().matchAll(/cached: (\d+)/g)].reduce((sum, match) => sum + Number(match[1]), 0);
  const lz4 = [...agent.log().matchAll(/ tiles: (\d+)/g)].reduce((sum, match) => sum + Number(match[1]), 0);
  console.log(`INFO: tile cache: ${cached} of ${cached + lz4} tiles from the cache (${Math.round((100 * cached) / Math.max(1, cached + lz4))} %)`);
  check(times.length > 0, `the encoder sent ${times.length} frames: p50 ${times[Math.floor(times.length / 2)]} µs, p95 ${times[Math.floor(times.length * 0.95)]} µs, ${Math.round(bytes.reduce((a, b) => a + b, 0) / 1024)} KiB`);

  // 5. The close button asks the app to close, and the app stops.
  await window.getByRole("button", { name: "Close" }).click();
  const end = Date.now() + TIMEOUT_MS;
  while (exited === null && Date.now() < end) await new Promise((resolve) => setTimeout(resolve, 100));
  check(exited === 0, "the close button stops the app");

  // 6. A photo goes as lossy tiles first, and the lossless pass makes it exact (F028).
  const viewer = "/usr/bin/eog";
  const photo = "/usr/share/backgrounds/gnome/adwaita-l.jpg";
  if (existsSync(viewer) && existsSync(photo)) {
    const eog = spawn(viewer, [photo], {
      env: { ...process.env, WAYLAND_DISPLAY: wayland, GDK_BACKEND: "wayland", DBUS_SESSION_BUS_ADDRESS: "disabled:", DISPLAY: "" },
      stdio: "ignore",
    });
    children.push(eog);
    await page.locator(".wm-window").filter({ has: page.locator(".linux-app") }).waitFor({ timeout: 30_000 });
    const sum = (name) => [...agent.log().matchAll(new RegExp(`${name}: (\\d+)`, "g"))].reduce((total, match) => total + Number(match[1]), 0);
    const until = Date.now() + 20_000;
    while ((sum("lossy") === 0 || sum("refined") < sum("lossy")) && Date.now() < until) await page.waitForTimeout(200);
    check(sum("lossy") > 0, `the photo went as ${sum("lossy")} lossy tiles`);
    check(sum("refined") >= sum("lossy"), `the lossless pass made ${sum("refined")} tiles exact`);
    if (SHOTS) await page.screenshot({ path: join(SHOTS, "surface-photo.png") });
  } else {
    console.log("INFO: no eog or no photo, the lossy step is not tested");
  }
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
