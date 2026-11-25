// camera-load.mjs - the camera must not delay the desktop (F118, ADR-058)
//
// Usage: npm --prefix spikes/e2e run camera-load
//
// The script starts farglassd in dev_user mode and the agent. For each case it
// measures 30 echo round trips of the Echo app with the camera off, then with
// the camera on while a host Chromium reads it through the camera portal (as
// camera.mjs does). The echo channel goes the same way as the keys and the
// clicks of the desktop. The cases:
//
// - WebTransport.
// - WebSocket.
// - WebSocket through a relay with an uplink of 1 Mbit/s and a queue of 32 KiB,
//   as on a slow home line. The camera then has less room than its bit rate.
//
// A case passes if the 95th percentile with the camera on is at most 30 ms
// above the one with the camera off. The script prints the bit rate and the
// frame rate that the camera settles on.
//
// CASE=NAME runs one case only, for example CASE=WebSocket.
//
// The script uses a release build of farglassd and the agent, as on a host.
//
// Exit status: 0 if all cases pass, 1 if one fails. The script skips the test
// with no PipeWire, no dbus-run-session, or no desktop portal.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, echo, mediaItem, start, TIMEOUT_MS } from "./lib.mjs";

/** The extra 95th percentile that the camera can add, in ms. */
const BOUND_MS = 30;

let agentLog = () => "";
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let failed = false;

/** Stops `child` and all processes of its group: portal.sh stops its private bus and portal then. */
function stopGroup(child, signal = "SIGTERM") {
  try {
    process.kill(-child.pid, signal);
  } catch {
    // The child leads no group of its own, or the group ended.
  }
  child.kill(signal);
}

/** Returns the number of PipeWire nodes with the name farglass-camera: the camera node of each running agent. */
function cameraNodes() {
  const nodes = JSON.parse(execFileSync("pw-dump", { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }));
  return nodes.filter((o) => o.type === "PipeWire:Interface:Node" && o.info?.props?.["node.name"] === "farglass-camera").length;
}

// An interrupted run stops its children too: a camera node that stays confuses the next run.
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    for (const child of children) stopGroup(child, "SIGKILL");
    process.exit(1);
  });
}

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

/**
 * A TCP relay to `target` that sends from the browser to farglassd at most `rate` bytes each second, as the uplink of a
 * slow line. Its queue holds at most `queue` bytes, then TCP stops the browser, as a home router does.
 */
async function slowRelay(target, rate, queue) {
  const [host, port] = [target.slice(0, target.lastIndexOf(":")), Number(target.slice(target.lastIndexOf(":") + 1))];
  const server = net.createServer((client) => {
    const upstream = net.connect(port, host);
    for (const socket of [client, upstream]) socket.on("error", () => {});
    upstream.pipe(client);
    const chunks = [];
    let queued = 0;
    client.on("data", (chunk) => {
      chunks.push(chunk);
      queued += chunk.length;
      if (queued > queue) client.pause();
    });
    const tick = 5;
    const timer = setInterval(() => {
      let budget = (rate * tick) / 1000;
      while (budget > 0 && chunks.length) {
        const chunk = chunks[0];
        const part = chunk.subarray(0, Math.ceil(budget));
        upstream.write(part);
        budget -= part.length;
        queued -= part.length;
        if (part.length === chunk.length) chunks.shift();
        else chunks[0] = chunk.subarray(part.length);
      }
      if (queued <= queue) client.resume();
    }, tick);
    const stop = () => {
      clearInterval(timer);
      client.destroy();
      upstream.destroy();
    };
    client.on("close", stop);
    upstream.on("close", stop);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { port: server.address().port, close: () => server.close() };
}

/** Returns the 50th and 95th percentile of `count` echo round trips, as the Echo app measures them in the page. */
async function echoTimes(page, label, count = 30) {
  const times = [];
  for (let i = 0; i < count; i++) {
    const text = `${label} ${i} ${Date.now()}`;
    const field = page.getByLabel("Message");
    await field.fill(text);
    await field.press("Enter");
    const line = page.locator('.echo-log li[data-kind="echo"]').filter({ hasText: text });
    await line.waitFor({ timeout: TIMEOUT_MS });
    times.push(Number((await line.textContent()).match(/\(([\d.]+) ms\)/)?.[1]));
    await page.waitForTimeout(50);
  }
  times.sort((a, b) => a - b);
  return { p50: times[Math.floor(count / 2)], p95: times[Math.floor(count * 0.95)] };
}

/** Starts the host Chromium on the camera portal. It keeps the camera open for `keep` ms. */
function hostChromium(keep) {
  const child = spawn(
    "../camera/portal.sh",
    ["farglass", "node", "../camera/browser.mjs", "chromium", "farglass browser camera", "--enable-features=WebRtcPipeWireCamera"],
    { cwd: import.meta.dirname, env: { ...process.env, GRANT: "yes", KEEP_MS: String(keep), PORTAL_LOG: join(state, "portal.log") }, stdio: ["ignore", "pipe", "pipe"], detached: true },
  );
  children.push(child);
  let out = "";
  child.stdout.on("data", (chunk) => (out += chunk));
  child.stderr.on("data", (chunk) => (out += chunk));
  const done = new Promise((resolve) => child.on("close", resolve));
  return { child, done, output: () => out };
}

/** Measures one case: the shell at `url`, with or without WebTransport. Returns the result line. */
async function measure(name, url, { noWebTransport }) {
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 } });
  await context.grantPermissions(["camera"], { origin: new URL(url).origin });
  if (noWebTransport) {
    await context.addInitScript(() => {
      delete globalThis.WebTransport;
    });
  }
  const page = await context.newPage();
  page.on("console", (message) => message.type() === "error" && console.log(`INFO: page error: ${message.text()}`));
  page.on("pageerror", (error) => console.log(`INFO: page error: ${error.message}`));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  await echo(page, `hello ${name}`);
  const off = await echoTimes(page, `off ${name}`);

  const button = page.locator(".panel-camera");
  await button.waitFor({ state: "attached", timeout: TIMEOUT_MS });
  await (await mediaItem(page, ".panel-camera")).click();
  await page.waitForFunction(() => document.querySelector(".panel-camera")?.textContent === "Cam ready", null, { timeout: TIMEOUT_MS });
  const host = hostChromium(120_000);
  try {
    await page.waitForFunction(() => document.querySelector(".panel-camera")?.dataset.state === "on", null, { timeout: 60_000 });
  } catch (error) {
    console.log(`INFO: host Chromium output: ${host.output().slice(-3000)}`);
    try {
      console.log(`INFO: portal log: ${readFileSync(join(state, "portal.log"), "utf8").slice(-2000)}`);
    } catch {
      console.log("INFO: no portal log");
    }
    try {
      const rows = execFileSync("ps", ["-eo", "pid,ppid,stat,wchan:32,etime,args"], { encoding: "utf8" }).split("\n").slice(1);
      const tree = new Set([host.child.pid]);
      for (let grew = true; grew; ) {
        grew = false;
        for (const row of rows) {
          const [pid, ppid] = row.trim().split(/\s+/).map(Number);
          if (tree.has(ppid) && !tree.has(pid)) {
            tree.add(pid);
            grew = true;
          }
        }
      }
      const lines = rows.filter((row) => tree.has(Number(row.trim().split(/\s+/)[0]))).map((row) => row.slice(0, 160));
      console.log(`INFO: host processes:\n${lines.join("\n")}`);
    } catch (error) {
      console.log(`INFO: no host processes: ${error.message}`);
    }
    console.log(`INFO: agent camera log: ${agentLog().split("\n").filter((l) => /camera/.test(l)).slice(-10).join(" | ")}`);
    throw error;
  }
  // The rate of the camera settles on the link.
  await page.waitForTimeout(15_000);
  const on = await echoTimes(page, `on ${name}`);
  const rate = await button.evaluate((el) => ({ bitrate: Number(el.dataset.bitrate), fps: Number(el.dataset.fps) }));
  stopGroup(host.child);
  await host.done;
  await (await mediaItem(page, ".panel-camera")).click();
  await context.close();
  const line = `${name}: echo p50/p95 ${off.p50}/${off.p95} ms with the camera off, ${on.p50}/${on.p95} ms on; camera ${Math.round(rate.bitrate / 1000)} kbit/s at ${rate.fps} fps`;
  console.log(`INFO: ${line}`);
  check(on.p95 <= off.p95 + BOUND_MS, `${name}: the camera adds at most ${BOUND_MS} ms to the 95th percentile of the echo`);
  return line;
}

try {
  try {
    execFileSync("pw-cli", ["info", "0"], { stdio: "ignore" });
    execFileSync("dbus-run-session", ["--", "true"], { stdio: "ignore" });
  } catch {
    console.log("SKIP: no PipeWire, or no dbus-run-session");
    process.exit(0);
  }
  if (!existsSync("/usr/libexec/xdg-desktop-portal")) {
    console.log("SKIP: no xdg-desktop-portal");
    process.exit(0);
  }
  if (cameraNodes() > 0) {
    throw new Error("A different agent has a farglass-camera node: stop it first, else the host Chromium can read that node.");
  }
  // A release build, as on a host: the decode of a debug build is many times slower.
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
  const agent = start(
    "farglass-session",
    `${bin}/farglass-session`,
    [],
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: join(state, "wayland-0"), GSETTINGS_BACKEND: "memory" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;
  agentLog = () => agent.log();

  browser = await chromium.launch({ args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"] });
  const results = [];
  for (const [name, run] of [
    ["WebTransport", () => measure("WebTransport", url, { noWebTransport: false })],
    ["WebSocket", () => measure("WebSocket", url, { noWebTransport: true })],
    [
      "WebSocket at 1 Mbit/s up",
      async () => {
        const relay = await slowRelay(new URL(url).host, 125_000, 32 * 1024);
        try {
          return await measure("WebSocket at 1 Mbit/s up", `https://127.0.0.1:${relay.port}/`, { noWebTransport: true });
        } finally {
          relay.close();
        }
      },
    ],
  ]) {
    // CASE=name runs one case only, for a test of the script itself.
    if (process.env.CASE && process.env.CASE !== name) continue;
    try {
      results.push(await run());
    } catch (error) {
      failed = true;
      console.error(`FAIL: ${name}: ${error.message}`);
    }
  }
  console.log("MEASURE:");
  for (const line of results) console.log(`  ${line}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) stopGroup(child, "SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
