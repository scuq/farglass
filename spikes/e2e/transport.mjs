// transport.mjs - end-to-end test of WebTransport and the WebSocket fallback in Chromium
//
// Usage: npm --prefix spikes/e2e run transport
//
// The script starts farglassd in dev_user mode and the agent. The shell must
// connect through WebTransport with the pinned certificate hash, and the echo
// channel and a terminal must work on it. A browser with no WebTransport, and
// a farglassd with http3 = false, must give the WebSocket. The script also
// measures the echo round trip during terminal output on each transport.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, echo, launch, start, TIMEOUT_MS } from "./lib.mjs";

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

/** Starts farglassd and an agent with `extra` lines in a configuration file. Returns the URL. */
async function stack(name, extra = "") {
  const dir = join(state, name);
  const socket = join(dir, "agent.sock");
  const config = join(state, `${name}.toml`);
  writeFileSync(config, extra);
  const daemon = start(
    `farglassd ${name}`,
    "target/debug/farglassd",
    ["--config", config, "--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", dir, "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start(`agent ${name}`, "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;
  return url;
}

/** Opens the shell at `url` and returns the page and the transport that the About app shows. */
async function open(url, { noWebTransport = false } = {}) {
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 } });
  if (noWebTransport) {
    await context.addInitScript(() => {
      delete globalThis.WebTransport;
    });
  }
  const page = await context.newPage();
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  await launch(page, "About farglass");
  // The desktop is the same in each tab of the user, thus an earlier About window can be there too.
  const field = page.locator('[data-field="transport"]').first();
  await field.waitFor({ timeout: TIMEOUT_MS });
  return { page, transport: await field.textContent() };
}

/** Sends `text` on the echo channel with Enter and returns the round trip in milliseconds. */
async function echoKey(page, text) {
  const field = page.getByLabel("Message");
  await field.fill(text);
  await field.press("Enter");
  const line = page.locator('.echo-log li[data-kind="echo"]').filter({ hasText: text });
  await line.waitFor({ timeout: TIMEOUT_MS });
  return Number((await line.textContent()).match(/\(([\d.]+) ms\)/)?.[1]);
}

/** Returns the 50th and 95th percentile of echo round trips while a terminal prints output. */
async function echoUnderLoad(page) {
  if ((await page.locator(".term-app").count()) === 0) {
    await launch(page, "Terminal");
  }
  // Tile the windows: no window covers a different one.
  await page.getByRole("button", { name: "Tile" }).click();
  const term = page.locator('.term-app[data-status="running"]').last();
  await term.waitFor({ timeout: TIMEOUT_MS });
  await term.click();
  await page.keyboard.type("for i in 1 2 3 4 5 6 7 8; do head -c 4000000 /dev/zero | od -An -v | head -c 3000000; done; echo load-done");
  await page.keyboard.press("Enter");
  const times = [];
  for (let i = 0; i < 20; i++) {
    times.push(await echoKey(page, `load ${i} ${Date.now()}`));
  }
  times.sort((a, b) => a - b);
  return [times[Math.floor(times.length / 2)], times[Math.floor(times.length * 0.95)]];
}

try {
  buildAll();
  browser = await chromium.launch();

  // 1. WebTransport by default.
  const url = await stack("quic");
  const { page, transport } = await open(url);
  check(transport === "WebTransport (HTTP/3)", "the shell connects through WebTransport with the pinned certificate hash");
  const time = await echo(page, "hello over WebTransport");
  check(Number(time) > 0, `the echo channel works on WebTransport, round trip ${time} ms`);
  await launch(page, "Terminal");
  await page.getByRole("button", { name: "Tile" }).click();
  const term = page.locator('.term-app[data-status="running"]').last();
  await term.waitFor({ timeout: TIMEOUT_MS });
  await term.click();
  await page.keyboard.type("echo wt-$((40+2))");
  await page.keyboard.press("Enter");
  await page.waitForFunction(
    () => [...document.querySelectorAll(".term-app")].some((el) => /^wt-42$/m.test(el.farglassTerminal?.text() ?? "")),
    null,
    { timeout: TIMEOUT_MS },
  );
  check(true, "a terminal works on WebTransport");
  const [quic50, quic95] = await echoUnderLoad(page);
  console.log(`INFO: echo during terminal output on WebTransport: p50 ${quic50} ms, p95 ${quic95} ms`);

  // 2. A browser with no WebTransport uses the WebSocket.
  const fallback = await open(url, { noWebTransport: true });
  check(fallback.transport === "WebSocket", "a browser with no WebTransport uses the WebSocket");
  const [ws50, ws95] = await echoUnderLoad(fallback.page);
  console.log(`INFO: echo during terminal output on the WebSocket: p50 ${ws50} ms, p95 ${ws95} ms`);

  // 3. A farglassd with no HTTP/3 gives the WebSocket.
  const tcpOnly = await stack("tcp", "http3 = false\n");
  const off = await open(tcpOnly);
  check(off.transport === "WebSocket", "with http3 = false, the shell uses the WebSocket");
  await echo(off.page, "hello over the WebSocket");
  check(true, "the echo channel works after the fallback");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
