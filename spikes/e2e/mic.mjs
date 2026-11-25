// mic.mjs - end-to-end test of the microphone for the apps (F114)
//
// Usage: npm --prefix spikes/e2e run mic
//
// The script starts farglassd in dev_user mode and the agent, which adds its
// microphone source to the PipeWire server of the user. Chromium runs with
// its fake microphone, a tone of beeps. A click on the Mic button sends the
// microphone to the agent. pw-record records from the source farglass-mic,
// as an app of the web session does, and must get the beeps. A second click
// stops the microphone, and the source gives silence again.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test with no PipeWire or no pw-record.

import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, mediaItem, start, TIMEOUT_MS } from "./lib.mjs";

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

/** Records `ms` of the source farglass-mic. Returns the peak of the samples and the number of samples. */
function record(ms) {
  return new Promise((resolve) => {
    const recorder = spawn("pw-record", ["--raw", "--target", "farglass-mic", "--format", "f32", "--rate", "48000", "--channels", "1", "-"], {
      stdio: ["ignore", "pipe", "ignore"],
    });
    children.push(recorder);
    const chunks = [];
    recorder.stdout.on("data", (chunk) => chunks.push(chunk));
    setTimeout(() => recorder.kill("SIGTERM"), ms);
    recorder.on("close", () => {
      const bytes = Buffer.concat(chunks);
      let peak = 0;
      for (let at = 0; at + 4 <= bytes.length; at += 4) peak = Math.max(peak, Math.abs(bytes.readFloatLE(at)));
      resolve({ peak, samples: Math.floor(bytes.length / 4) });
    });
  });
}

try {
  try {
    execFileSync("pw-cli", ["info", "0"], { stdio: "ignore" });
    execFileSync("pw-record", ["--version"], { stdio: "ignore" });
  } catch {
    console.log("SKIP: no PipeWire, or no pw-record");
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
  check(/the microphone source of the session is in PipeWire/.test(agent.log()), "the agent adds its microphone source to PipeWire");

  browser = await chromium.launch({ args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"] });
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1200, height: 800 } });
  await context.grantPermissions(["microphone"], { origin: url });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  const button = page.locator(".panel-mic");
  await button.waitFor({ state: "attached", timeout: TIMEOUT_MS });
  check((await button.textContent()) === "Mic off", "the panel shows the microphone button, off");

  const before = await record(1500);
  check(before.samples > 0 && before.peak === 0, `the source gives silence while the microphone is off (${before.samples} samples)`);

  await (await mediaItem(page, ".panel-mic")).click();
  await page.waitForFunction(() => document.querySelector(".panel-mic")?.dataset.state === "on", null, { timeout: TIMEOUT_MS });
  check(true, "a click turns the microphone on");
  check(/event=microphone result=accepted reason=start/.test(agent.log()), "the agent logs the start in the security log");
  const during = await record(3000);
  check(during.peak > 0.01, `an app records the fake microphone of the browser (peak ${during.peak.toFixed(3)})`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "mic.png") });

  check((await page.locator(".panel-media").textContent()) === "Mic on", "the panel button shows Mic on");
  await (await mediaItem(page, ".panel-mic")).click();
  check((await button.textContent()) === "Mic off", "a second click turns the microphone off");
  await page.waitForTimeout(300);
  const after = await record(1500);
  check(after.peak === 0, "the source gives silence again");
  check(/event=microphone result=accepted reason=stop/.test(agent.log()), "the agent logs the stop");
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
