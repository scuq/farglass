// echo.mjs - end-to-end test of the echo channel in dev_user mode
//
// Usage: npm --prefix spikes/e2e run echo
//
// The script builds farglassd, farglass-session, and the shell. It starts
// farglassd on a free loopback port in dev_user mode as the current user, and
// starts the agent. Then Chromium opens the shell in two tabs, sends a message
// on the echo channel of each tab, and waits for the echo.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, echo, start } from "./lib.mjs";

const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const socket = join(state, "agent.sock");
const children = [];
let browser;
let failed = false;

try {
  buildAll();

  const uid = String(process.getuid());
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", socket, "--dev-user", uid],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  console.log(`farglassd listens on ${url}`);

  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;
  console.log("the agent is connected");

  browser = await chromium.launch();
  // farglassd uses a self-signed certificate in this test.
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const first = await context.newPage();
  const second = await context.newPage();
  await Promise.all([first.goto(url), second.goto(url)]);

  const times = await Promise.all([echo(first, "hello from tab 1"), echo(second, "hello from tab 2")]);
  console.log(`PASS: two tabs got their echo, round trip ${times.join(" ms and ")} ms`);

  // Each tab gets only its own echo.
  const crossed = await first.locator('.echo-log li[data-kind="echo"]').filter({ hasText: "tab 2" }).count();
  if (crossed !== 0) {
    throw new Error("Tab 1 got the echo of tab 2.");
  }
  console.log("PASS: each tab got only its own echo");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) {
    child.kill("SIGTERM");
  }
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
