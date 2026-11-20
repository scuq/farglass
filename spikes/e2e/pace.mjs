// pace.mjs - end-to-end test of the adaptive frame rate of Linux app windows (F048)
//
// Usage: npm --prefix spikes/e2e run pace
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket in a temporary directory. fixtures/animation.py --small draws a
// small area in each frame, too small for video: an app that draws all the
// time. The app prints its frames each second. The script tests that:
//
// - The app gets the idle rate when no input comes for 2 s.
// - Input over the window gives the full rate again.
// - The app draws no frame when no browser shows its window.
// - A window with a video region keeps the full rate with no input.
//
// It runs the steps with FARGLASS_IDLE_FPS=0 (no limit) too, and prints the
// host CPU of the app and of the agent in the idle time, with and without the limit.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { buildAll, raise, start, TIMEOUT_MS } from "./lib.mjs";

const FIXTURE = join(dirname(fileURLToPath(import.meta.url)), "fixtures", "animation.py");
/** The time of each measurement. */
const MEASURE_MS = 4000;
let failed = false;

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

/** Returns the CPU time of the process `pid` in milliseconds, from /proc. */
function cpu(pid) {
  const fields = readFileSync(`/proc/${pid}/stat`, "utf8").split(") ")[1].split(" ");
  // utime and stime are the fields 14 and 15, in clock ticks of 10 ms.
  return (Number(fields[11]) + Number(fields[12])) * 10;
}

/** Starts a scenario: farglassd, the agent with `idleFps`, a browser page, and the app. */
async function setup(idleFps, args) {
  const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
  const children = [];
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
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, FARGLASS_IDLE_FPS: String(idleFps), RUST_LOG: "info" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;
  const browser = await chromium.launch();
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1200, height: 800 } })).newPage();
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
  const app = spawn("python3", [FIXTURE, "--fps", ...args], { env, stdio: ["ignore", "pipe", "ignore"] });
  children.push(app);
  const rates = [];
  app.stdout.on("data", (chunk) => {
    for (const match of String(chunk).matchAll(/fps (\d+)/g)) rates.push(Number(match[1]));
  });
  const window = page.locator('.wm-window[aria-label="Animation"]');
  await window.waitFor({ timeout: 60_000 });
  await raise(page, window);
  // The frames of the app in the next `ms`: the mean of the reports of each second.
  const fps = async (ms = MEASURE_MS) => {
    const from = rates.length;
    await page.waitForTimeout(ms);
    const seen = rates.slice(from + 1);
    return seen.length ? Math.round(seen.reduce((a, b) => a + b, 0) / seen.length) : 0;
  };
  const close = async () => {
    await browser.close();
    for (const child of children) child.kill("SIGTERM");
    rmSync(state, { recursive: true, force: true });
  };
  return { page, window, app, agent: agent.child, fps, close };
}

/** Runs the steps with the idle rate `idleFps`. Returns the CPU time of the app and of the agent in the idle time. */
async function scenario(idleFps) {
  const run = await setup(idleFps, ["--small"]);
  try {
    const { page, window, fps } = run;
    // 1. With no input for 2 s, the app gets the idle rate.
    await page.mouse.move(1150, 750);
    await page.waitForTimeout(2500);
    const before = { app: cpu(run.app.pid), agent: cpu(run.agent.pid) };
    const idle = await fps();
    const used = { app: cpu(run.app.pid) - before.app, agent: cpu(run.agent.pid) - before.agent };
    if (idleFps > 0) {
      check(idle > 0 && idle <= idleFps + 2, `with no input, the app draws at the idle rate (${idle} fps, limit ${idleFps})`);
    } else {
      check(idle >= 30, `with no limit, the app draws at the full rate (${idle} fps)`);
    }

    // 2. Input over the window gives the full rate.
    const box = await window.locator("canvas").first().boundingBox();
    const from = Date.now();
    const moving = (async () => {
      for (let i = 0; Date.now() - from < MEASURE_MS; i++) {
        await page.mouse.move(box.x + 60 + (i % 40), box.y + 60);
        await page.waitForTimeout(50);
      }
    })();
    const active = await fps();
    await moving;
    check(active >= 30, `input over the window gives the full rate (${active} fps)`);

    // 3. With no browser, the app draws no frame.
    if (idleFps > 0) {
      await page.close();
      await new Promise((resolve) => setTimeout(resolve, 1500));
      const gone = await (async () => {
        const app = cpu(run.app.pid);
        await new Promise((resolve) => setTimeout(resolve, MEASURE_MS));
        return cpu(run.app.pid) - app;
      })();
      check(gone <= 100, `with no browser, the app draws no frame (${gone} ms CPU in ${MEASURE_MS / 1000} s)`);
    }
    return used;
  } finally {
    await run.close();
  }
}

try {
  buildAll();
  const limited = await scenario(10);
  const free = await scenario(0);
  console.log(
    `INFO: host CPU in ${MEASURE_MS / 1000} s with no input: app ${limited.app} ms, agent ${limited.agent} ms with the limit; ` +
      `app ${free.app} ms, agent ${free.agent} ms with no limit`,
  );
  check(limited.app + limited.agent < free.app + free.agent, "the limit lowers the host CPU of an app that draws all the time");

  // 4. A video region keeps the full rate with no input.
  const run = await setup(10, []);
  try {
    await run.page.mouse.move(1150, 750);
    await run.page.waitForTimeout(3000);
    const rate = await run.fps();
    check(rate >= 30, `a window with a video region keeps the full rate with no input (${rate} fps)`);
  } finally {
    await run.close();
  }
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
}

process.exit(failed ? 1 : 0);
