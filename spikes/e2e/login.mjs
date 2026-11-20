// login.mjs - end-to-end test of the PAM login in the shell
//
// Usage: npm --prefix spikes/e2e run login
//
// The script starts farglass-login with a PAM directory that needs no root:
// pam_exec asks for the password and accepts "secret". It starts farglassd
// with no dev_user, and the agent. Then Chromium logs in through the login
// screen, first with a wrong password, then with the correct one, and uses
// the echo channel. It locks the session and unlocks it in a different tab.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { PRIMARY_GROUP, buildAll, echo, logout, relay, start, TIMEOUT_MS } from "./lib.mjs";

const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let failed = false;

/** Writes a PAM directory with a stack that accepts the password "secret". */
function pamDirectory() {
  const dir = join(state, "pam.d");
  mkdirSync(dir);
  const check = join(state, "check.sh");
  writeFileSync(check, '#!/bin/sh\nIFS= read -r password; [ "$password" = secret ]\n');
  chmodSync(check, 0o755);
  writeFileSync(
    join(dir, "farglass"),
    `auth required pam_exec.so expose_authtok quiet ${check}\naccount required pam_permit.so\n`,
  );
  return dir;
}

/** Answers the prompt with the label `label` on the login screen. */
async function answer(page, label, text) {
  const field = page.getByLabel(label, { exact: true });
  await field.waitFor({ timeout: TIMEOUT_MS });
  await field.fill(text);
  await page.getByRole("button", { name: "Continue" }).click();
  return field;
}

try {
  buildAll();
  const user = userInfo().username;
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
    [
      "--listen", "127.0.0.1:0",
      "--web-root", "web/dist",
      "--state-dir", state,
      "--agent-socket", agentSocket,
      "--login-socket", loginSocket,
      "--allow-user-agents",
    ],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, direct] = await daemon.match;
  // The browser connects through a relay, thus the test can break the connection.
  const link = await relay(new URL(direct).host);
  const url = `https://localhost:${link.port}/`;

  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: agentSocket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const cspViolations = [];
  context.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) {
      cspViolations.push(message.text());
    }
  });
  const page = await context.newPage();
  await page.goto(url);
  await page.getByRole("heading", { name: /Log in to/ }).waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: the shell shows the login screen");

  // PAM asks for the username, because farglassd gives none.
  await answer(page, "Login", user);
  const password = await answer(page, "Password", "guess");
  if ((await password.getAttribute("type")) !== "password") {
    throw new Error("The password field shows the password.");
  }
  await page.getByRole("alert").filter({ hasText: "The login failed." }).waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: a wrong password fails");

  await page.getByRole("button", { name: "Try again" }).click();
  await answer(page, "Login", user);
  await answer(page, "Password", "secret");
  await page.getByText(`Logged in as ${user}`).waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: the correct password logs in");

  const time = await echo(page, "hello after the login");
  console.log(`PASS: the echo channel works after the login, round trip ${time} ms`);

  // The session cookie: a reload and a new tab skip the login.
  await page.waitForResponse((response) => response.url().endsWith("/session"), { timeout: TIMEOUT_MS }).catch(() => {});
  const cookies = await context.cookies();
  const session = cookies.find((cookie) => cookie.name === "__Host-farglass");
  if (!session || !session.httpOnly || !session.secure || session.sameSite !== "Strict") {
    throw new Error(`The session cookie is missing or not protected: ${JSON.stringify(session)}`);
  }
  if ((await page.evaluate(() => document.cookie)).includes("__Host-farglass")) {
    throw new Error("A script can read the session cookie.");
  }
  console.log("PASS: the session cookie is HttpOnly, Secure, and SameSite=Strict");

  await page.reload();
  await page.getByText(`Logged in as ${user}`).waitFor({ timeout: TIMEOUT_MS });
  // The state store of the agent gives the echo log back after the reload.
  await page.locator(".echo-log li").filter({ hasText: "hello after the login" }).first().waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: the echo log comes back from the state store after a reload");
  const tab = await context.newPage();
  await tab.goto(url);
  await tab.getByText(`Logged in as ${user}`).waitFor({ timeout: TIMEOUT_MS });
  await echo(tab, "hello from a new tab");
  console.log("PASS: a reload and a new tab skip the login");

  // A break of the network: the shell waits and connects again with no login.
  link.cut();
  await page.getByRole("heading", { name: "No connection to the host" }).waitFor({ timeout: TIMEOUT_MS });
  link.restore();
  await page.getByText(`Logged in as ${user}`).waitFor({ timeout: 30_000 });
  await echo(page, "hello after the break");
  console.log("PASS: after a break of the network, the shell connects again with no login");

  // Lock in one tab locks each tab. The lock screen asks only for the password.
  await page.locator(".panel-session").click();
  await page.getByRole("menuitem", { name: "Lock" }).click();
  for (const each of [page, tab]) {
    await each.getByRole("heading", { name: "Locked" }).waitFor({ timeout: TIMEOUT_MS });
    if ((await each.locator(".wm").count()) !== 0) {
      throw new Error("The lock screen keeps the desktop in the page.");
    }
  }
  console.log("PASS: Lock in the session menu shows the lock screen in each tab, with no desktop in the page");
  await answer(tab, "Password", "guess");
  await tab.getByRole("alert").filter({ hasText: "The login failed." }).waitFor({ timeout: TIMEOUT_MS });
  await page.reload();
  await page.getByRole("heading", { name: "Locked" }).waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: a wrong password does not unlock, and a reload keeps the lock");
  await tab.getByRole("button", { name: "Try again" }).click();
  await answer(tab, "Password", "secret");
  for (const each of [tab, page]) {
    await each.getByText(`Logged in as ${user}`).waitFor({ timeout: TIMEOUT_MS });
  }
  await page.locator(".echo-log li").filter({ hasText: "hello after the break" }).first().waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: the password of the user unlocks each tab, and the desktop comes back");

  // A logout in one tab reaches the other tab.
  await logout(page, user);
  for (const each of [page, tab]) {
    await each.getByText("You logged out.").waitFor({ timeout: TIMEOUT_MS });
    await each.getByRole("heading", { name: /Log in to/ }).waitFor({ timeout: TIMEOUT_MS });
  }
  console.log("PASS: the logout reaches each tab of the user");

  // A different browser profile has no cookie and must log in.
  const other = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await other.goto(url);
  await other.getByRole("heading", { name: /Log in to/ }).waitFor({ timeout: TIMEOUT_MS });
  console.log("PASS: a browser with no cookie gets the login screen");

  if (cspViolations.length > 0) {
    throw new Error(`The page broke the Content Security Policy: ${cspViolations.join("; ")}`);
  }
  console.log("PASS: no Content Security Policy violation");
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
