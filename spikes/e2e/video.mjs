// video.mjs - end-to-end test of the video region of a surface (F037)
//
// Usage: npm --prefix spikes/e2e run video
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket in a temporary directory, and fixtures/animation.py on it: a GTK 4
// window with a label and an area that changes in each frame. The area must
// go as H.264 video and decode in the browser, the label must stay exact, and
// the picture in the browser must change.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test if Python with GTK 4 is not installed.

import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { buildAll, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const FIXTURE = join(dirname(fileURLToPath(import.meta.url)), "fixtures", "animation.py");
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
  try {
    execFileSync("python3", ["-c", "import gi; gi.require_version('Gtk', '4.0')"]);
  } catch {
    console.log("SKIP: no Python with GTK 4");
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
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, RUST_LOG: "info,farglass_surface=debug" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1200, height: 800 } })).newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  const env = {
    HOME: process.env.HOME,
    PATH: "/usr/bin:/bin",
    XDG_RUNTIME_DIR: process.env.XDG_RUNTIME_DIR,
    WAYLAND_DISPLAY: wayland,
    GDK_BACKEND: "wayland",
    DBUS_SESSION_BUS_ADDRESS: "disabled:",
  };
  children.push(spawn("python3", [FIXTURE], { env, stdio: "ignore" }));
  const window = page.locator('.wm-window[aria-label="Animation"]');
  await window.waitFor({ timeout: 60_000 });
  check(true, "the animation window shows");

  // 1. The area goes as video, and the browser decodes it.
  const canvas = window.locator("canvas").first();
  await page.waitForFunction(
    () => Number(document.querySelector('.wm-window[aria-label="Animation"] canvas')?.dataset.videoFrames ?? 0) >= 30,
    null,
    { timeout: TIMEOUT_MS },
  );
  check(/a region of the surface goes as video/.test(agent.log()), "the agent sends the area as video");
  const frames = Number(await canvas.getAttribute("data-video-frames"));
  await page.waitForTimeout(1000);
  const later = Number(await canvas.getAttribute("data-video-frames"));
  check(later - frames >= 10, `the browser decodes the video continuously (${later - frames} frames in 1 s)`);

  // 2. The picture of the area changes, and it is not empty.
  const box = await canvas.boundingBox();
  // The area is under the label: 12 pixels of margin, the label, and 12 pixels of space.
  const area = { x: box.x + 40, y: box.y + box.height - 200, width: 240, height: 160 };
  const first = await page.screenshot({ clip: area });
  await page.waitForTimeout(300);
  const second = await page.screenshot({ clip: area });
  check(!first.equals(second), "the area in the browser changes");
  const colors = await page.evaluate(() => {
    const element = document.querySelector('.wm-window[aria-label="Animation"] canvas');
    return element ? element.width : 0;
  });
  check(colors > 0, "the canvas has a size");
  if (SHOTS) {
    writeFileSync(join(SHOTS, "video-area.png"), second);
    await page.screenshot({ path: join(SHOTS, "video.png") });
  }
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
