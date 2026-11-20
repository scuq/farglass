// pin.mjs - end-to-end test of the machine PIN before the web login (F059)
//
// Usage: npm --prefix spikes/e2e run pin
//
// The script sets a PIN with farglassd --set-pin, then starts farglass-login
// with a PAM directory that needs no root (pam_exec accepts the password
// "secret"), farglassd, and the agent. It tests that:
//
// - The shell shows the PIN screen first, and a wrong PIN shows an error.
// - The correct PIN shows the login screen, and the password logs in.
// - A reload with the session cookie needs no PIN.
// - farglassd writes each PIN result to the security log, with no PIN in it.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { PRIMARY_GROUP, buildAll, root, start, TIMEOUT_MS } from "./lib.mjs";

const PIN = "482913";
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

/** Writes a PAM directory with a stack that accepts the password "secret". */
function pamDirectory() {
  const dir = join(state, "pam.d");
  mkdirSync(dir);
  const script = join(state, "check.sh");
  writeFileSync(script, '#!/bin/sh\nIFS= read -r password; [ "$password" = secret ]\n');
  chmodSync(script, 0o755);
  writeFileSync(join(dir, "farglass"), `auth required pam_exec.so expose_authtok quiet ${script}\naccount required pam_permit.so\n`);
  return dir;
}

/** Answers the prompt with the label `label` on the login screen. */
async function answer(page, label, text) {
  const field = page.getByLabel(label, { exact: true });
  await field.waitFor({ timeout: TIMEOUT_MS });
  await field.fill(text);
  await page.getByRole("button", { name: "Continue" }).click();
}

try {
  buildAll();
  const user = userInfo().username;
  execFileSync(join(root, "target/debug/farglassd"), ["--state-dir", state, "--set-pin"], { input: `${PIN}\n`, stdio: ["pipe", "ignore", "ignore"] });
  const loginSocket = join(state, "login.sock");
  const agentSocket = join(state, "agent.sock");
  const login = start(
    "farglass-login",
    "target/debug/farglass-login",
    ["--socket", loginSocket, "--pam-dir", pamDirectory(), "--fail-delay-ms", "0", "--session", "none", "--allowed-group", PRIMARY_GROUP],
    {},
    /ready/,
  );
  children.push(login.child);
  await login.match;
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", agentSocket, "--login-socket", loginSocket, "--allow-user-agents"],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  let log = "";
  daemon.child.stderr.on("data", (chunk) => (log += chunk));
  const [, url] = await daemon.match;
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: agentSocket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();

  // 1. The PIN screen comes first.
  await page.goto(url);
  await page.getByLabel("PIN of this host").waitFor({ timeout: TIMEOUT_MS });
  check((await page.getByLabel("Login", { exact: true }).count()) === 0, "the PIN screen comes before the login screen");

  // 2. A wrong PIN fails.
  await answer(page, "PIN of this host", "000000");
  await page.getByRole("alert").filter({ hasText: "The PIN is not correct." }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "a wrong PIN shows an error");

  // 3. The correct PIN shows the login screen, and the password logs in.
  await answer(page, "PIN of this host", PIN);
  await answer(page, "Login", user);
  await answer(page, "Password", "secret");
  await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "the correct PIN shows the login screen, and the password logs in");

  // 4. A reload with the session cookie needs no PIN.
  await page.reload();
  await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "a reload with the session cookie needs no PIN");

  // 5. The security log has each result, and no PIN.
  check(/farglass::security: event=pin result=refused reason=wrong-pin addr=127\.0\.0\.1/.test(log), "the log has the wrong PIN");
  check(/farglass::security: event=pin result=accepted reason=- addr=127\.0\.0\.1/.test(log), "the log has the correct PIN");
  check(!log.includes(PIN) && !log.includes("000000"), "no PIN is in the log");
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
