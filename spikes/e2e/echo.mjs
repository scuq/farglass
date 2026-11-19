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

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright";

const root = resolve(import.meta.dirname, "../..");
const TIMEOUT_MS = 15_000;

/** Runs a build command and stops the script if it fails. */
function build(command, args) {
  console.log(`$ ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit status ${result.status}.`);
  }
}

/** Starts a program and resolves with the first match of `pattern` in its log. */
function start(name, command, args, env, pattern) {
  const child = spawn(command, args, {
    cwd: root,
    env: { ...process.env, RUST_LOG: "info", NO_COLOR: "1", ...env },
    stdio: ["ignore", "inherit", "pipe"],
  });
  let log = "";
  const match = new Promise((resolveMatch, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`${name} did not log ${pattern} in ${TIMEOUT_MS} ms:\n${log}`)),
      TIMEOUT_MS,
    );
    child.stderr.on("data", (chunk) => {
      log += chunk;
      const found = log.match(pattern);
      if (found) {
        clearTimeout(timer);
        resolveMatch(found);
      }
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`${name} stopped with exit status ${code}:\n${log}`));
    });
  });
  return { child, match, log: () => log };
}

/** Sends `text` on the echo panel of `page` and returns the round-trip time from the log line. */
async function echo(page, text) {
  await page.getByRole("status").filter({ hasText: "Echo channel open" }).waitFor({ timeout: TIMEOUT_MS });
  await page.getByLabel("Message").fill(text);
  await page.getByRole("button", { name: "Send" }).click();
  const line = page.locator('.echo-log li[data-kind="echo"]').filter({ hasText: text });
  await line.waitFor({ timeout: TIMEOUT_MS });
  return (await line.textContent()).match(/\(([\d.]+) ms\)/)?.[1];
}

const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const socket = join(state, "agent.sock");
const children = [];
let browser;
let failed = false;

try {
  build("cargo", ["build", "-p", "farglassd", "-p", "farglass-session"]);
  build("npm", ["--prefix", "web", "run", "build"]);

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
