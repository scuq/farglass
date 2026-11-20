// sound.mjs - end-to-end test of the sound of the apps (F038)
//
// Usage: npm --prefix spikes/e2e run sound
//
// The script starts farglassd in dev_user mode and the agent, which adds its
// sound sink to the PipeWire server of the user. fixtures/animation.py --tone
// plays a sine tone through GStreamer, with PULSE_SINK and PIPEWIRE_NODE as a
// web session gives them. The browser must get Opus packets, decode them, and
// play them: the panel shows the sound button, and the level of the
// worklet is the level of the tone.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test with no PipeWire or no Python with GTK 4 and GStreamer.

import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
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
    execFileSync("pw-cli", ["info", "0"], { stdio: "ignore" });
    execFileSync("python3", ["-c", "import gi; gi.require_version('Gtk', '4.0'); gi.require_version('Gst', '1.0')"]);
  } catch {
    console.log("SKIP: no PipeWire, or no Python with GTK 4 and GStreamer");
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
  check(/the sound sink of the session is in PipeWire/.test(agent.log()), "the agent adds its sink to PipeWire");

  browser = await chromium.launch({ args: ["--autoplay-policy=no-user-gesture-required"] });
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1200, height: 800 } })).newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  const button = page.locator(".panel-sound");
  await button.waitFor({ timeout: TIMEOUT_MS });
  check(true, "the panel shows the sound button");

  const env = {
    HOME: process.env.HOME,
    PATH: "/usr/bin:/bin",
    XDG_RUNTIME_DIR: process.env.XDG_RUNTIME_DIR,
    WAYLAND_DISPLAY: wayland,
    GDK_BACKEND: "wayland",
    DBUS_SESSION_BUS_ADDRESS: "disabled:",
    PULSE_SINK: "farglass",
    PIPEWIRE_NODE: "farglass",
  };
  children.push(spawn("python3", [FIXTURE, "--tone"], { env, stdio: "ignore" }));
  // A press is the user action that lets the browser start the sound.
  await page.mouse.click(900, 600);
  await page.waitForFunction(() => Number(document.querySelector(".panel-sound")?.dataset.packets ?? 0) >= 50, null, { timeout: 30_000 });
  check(true, "the browser gets Opus packets of the tone");
  await page.waitForFunction(() => document.querySelector(".panel-sound")?.dataset.playing === "true", null, { timeout: TIMEOUT_MS });
  await page.waitForTimeout(500);
  const level = Number(await button.getAttribute("data-level"));
  // The tone has the volume 0.3: its peak is near 0.3.
  check(level > 0.2 && level < 0.4, `the worklet plays the tone (peak ${level})`);

  await button.click();
  check((await button.getAttribute("aria-pressed")) === "true", "a click turns the sound off");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "sound.png") });
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
