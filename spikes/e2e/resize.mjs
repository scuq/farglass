// resize.mjs - end-to-end test of the local move and resize of a Linux window (F047)
//
// Usage: npm --prefix spikes/e2e run resize
//
// The script starts farglassd in dev_user mode, the agent with a Wayland socket,
// and gnome-calculator on it. A slow drag of the corner of the window, with
// pauses, must send one size to the app, at the end. During the drag, the
// window shows its last frame with its edges stretched into the new area. A
// size below the smallest size of the calculator ends with the frame at the
// size that the calculator keeps.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test if gnome-calculator is not installed.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, raise, start, TIMEOUT_MS } from "./lib.mjs";

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

try {
  if (!existsSync(APP)) {
    console.log("SKIP: gnome-calculator is not installed");
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
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, RUST_LOG: "info,farglass_session::surfaces=debug" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1400, height: 900 } })).newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  children.push(
    spawn(APP, [], {
      env: { ...process.env, WAYLAND_DISPLAY: wayland, GDK_BACKEND: "wayland", DBUS_SESSION_BUS_ADDRESS: "disabled:", DISPLAY: "" },
      stdio: "ignore",
    }),
  );
  const window = page.locator(".wm-window").filter({ has: page.locator(".linux-app") });
  await window.waitFor({ timeout: 30_000 });
  await page.waitForTimeout(1500);
  await raise(page, window);
  const sizes = () => (agent.log().match(/the shell gives a window a size/g) ?? []).length;

  // 1. A slow drag of the corner, with pauses: no size goes during it, and one size at the end.
  const before = await window.boundingBox();
  const before_sizes = sizes();
  // The handle of the corner reaches 4 px past the window.
  const corner = { x: before.x + before.width + 2, y: before.y + before.height + 2 };
  await page.mouse.move(corner.x, corner.y);
  await page.mouse.down();
  let shownEdges = false;
  for (let step = 1; step <= 6; step++) {
    await page.mouse.move(corner.x + step * 20, corner.y + step * 10, { steps: 4 });
    await page.waitForTimeout(250);
    shownEdges ||= await window.locator(".linux-edge").evaluateAll((all) => all.some((edge) => getComputedStyle(edge).display !== "none" && edge.width > 0));
  }
  check(sizes() === before_sizes, `no size goes to the app during the drag (${sizes() - before_sizes} sizes)`);
  check(shownEdges, "the edges of the last frame stretch into the new area during the drag");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "resize-drag.png") });
  await page.mouse.up();
  await page.waitForTimeout(1500);
  check(sizes() === before_sizes + 1, `one size goes at the end of the drag (${sizes() - before_sizes} sizes)`);
  const hidden = await window.locator(".linux-edge").evaluateAll((all) => all.every((edge) => getComputedStyle(edge).display === "none"));
  check(hidden, "the live content shows again after the drag");
  const after = await window.boundingBox();
  check(Math.abs(after.width - before.width - 120) <= 2 && Math.abs(after.height - before.height - 60) <= 2, `the window has the new size (${Math.round(after.width)}x${Math.round(after.height)})`);

  // 2. A size below the smallest size of the calculator: the frame takes the size of the app.
  const big = await window.boundingBox();
  await page.mouse.move(big.x + big.width + 2, big.y + big.height + 2);
  await page.mouse.down();
  await page.mouse.move(big.x + 260, big.y + 200, { steps: 8 });
  await page.mouse.up();
  await page.waitForTimeout(2000);
  const small = await window.boundingBox();
  if (SHOTS) writeFileSync(join(SHOTS, "resize-small.png"), await page.screenshot());
  // The frame did shrink, but not to 260 x 200: the calculator keeps its smallest size.
  check(small.width < big.width - 50 && small.width > 270 && small.height > 210, `the app keeps its smallest size, and the frame takes it (${Math.round(small.width)}x${Math.round(small.height)}, from ${Math.round(big.width)}x${Math.round(big.height)})`);
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
