// passkey.mjs - end-to-end test of passkeys as a second factor (F034)
//
// Usage: npm --prefix spikes/e2e run passkey
//
// The script starts farglass-login with a PAM directory that needs no root
// (pam_exec accepts the password "secret"), farglassd with no dev_user, and
// the agent. Chromium gets a virtual authenticator through the DevTools
// protocol. The script tests that:
//
// - The user adds a passkey in Settings > Passkeys and gets recovery codes.
// - The next login asks for the passkey after the password, and the passkey logs in.
// - A recovery code logs in one time, and a second use fails.
// - A change of the passkeys needs a fresh passkey first, and a wrong recovery code changes nothing (F053).
// - farglassd keeps the passkeys in a file that only it can read.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { chmodSync, mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { PRIMARY_GROUP, buildAll, launch, logout, start, TIMEOUT_MS } from "./lib.mjs";

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

/** Logs in with the username and the password, up to the passkey step. */
async function password(page, user) {
  await page.getByRole("heading", { name: /Log in to/ }).waitFor({ timeout: TIMEOUT_MS });
  await answer(page, "Login", user);
  await answer(page, "Password", "secret");
  await page.getByRole("heading", { name: "Second step: your passkey" }).waitFor({ timeout: TIMEOUT_MS });
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
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", agentSocket, "--login-socket", loginSocket],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, direct] = await daemon.match;
  // A passkey needs a host name: localhost, not 127.0.0.1.
  const url = `https://localhost:${new URL(direct).port}/`;
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: agentSocket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: { protocol: "ctap2", transport: "internal", hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true },
  });

  // 1. With no passkey, the password alone logs in.
  await page.goto(url);
  await page.getByRole("heading", { name: /Log in to/ }).waitFor({ timeout: TIMEOUT_MS });
  await answer(page, "Login", user);
  await answer(page, "Password", "secret");
  await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "with no passkey, the password alone logs in");

  // 2. Settings > Passkeys adds a passkey and shows the recovery codes one time.
  await launch(page, "Settings");
  const win = page.locator('.wm-window[aria-label="Settings"]');
  await win.locator(".settings-nav").getByRole("button", { name: "Passkeys", exact: true }).click();
  await win.getByText("You have no passkey.").waitFor({ timeout: TIMEOUT_MS });
  await win.getByLabel("Name of the new passkey").fill("Test key");
  await win.getByRole("button", { name: "Add a passkey" }).click();
  await win.getByRole("heading", { name: "Your recovery codes" }).waitFor({ timeout: TIMEOUT_MS });
  let codes = await win.locator(".settings-codes code").allTextContents();
  check(codes.length === 8 && codes.every((code) => /^[0-9a-f]{4}(-[0-9a-f]{4}){3}$/.test(code)), `the first passkey gives 8 recovery codes (${codes[0]})`);
  await win.getByRole("button", { name: "I saved them" }).click();
  await win.getByRole("cell", { name: "Test key", exact: true }).waitFor({ timeout: TIMEOUT_MS });
  await win.getByRole("cell", { name: "localhost", exact: true }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "the list shows the passkey with its host name");
  const file = join(state, "passkeys", `${process.getuid()}.json`);
  check((statSync(file).mode & 0o777) === 0o600, "farglassd keeps the passkeys in a file with mode 0600");

  // 2b. New recovery codes need a fresh passkey first (F053).
  const newCodes = async (window) => {
    page.once("dialog", (dialog) => void dialog.accept());
    await window.getByRole("button", { name: "Make new recovery codes" }).click();
  };
  const readCodes = async (window) => {
    await window.getByRole("heading", { name: "Your recovery codes" }).waitFor({ timeout: TIMEOUT_MS });
    const list = await window.locator(".settings-codes code").allTextContents();
    await window.getByRole("button", { name: "I saved them" }).click();
    return list;
  };
  await newCodes(win);
  await win.getByRole("heading", { name: "Confirm that it is you" }).waitFor({ timeout: TIMEOUT_MS });
  await win.getByRole("button", { name: "Confirm with my passkey" }).click();
  codes = await readCodes(win);
  check(codes.length === 8, "new recovery codes need a fresh passkey first");
  // In the next minutes, the same Settings window needs no new proof.
  await newCodes(win);
  codes = await readCodes(win);
  check((await win.getByRole("heading", { name: "Confirm that it is you" }).count()) === 0, "after the proof, the same window needs no new proof");
  // A new Settings window needs a new proof, and a wrong recovery code changes nothing.
  await win.getByRole("button", { name: "Close" }).first().click();
  await launch(page, "Settings");
  const again = page.locator('.wm-window[aria-label="Settings"]');
  await again.locator(".settings-nav").getByRole("button", { name: "Passkeys", exact: true }).click();
  page.once("dialog", (dialog) => void dialog.accept());
  await again.getByRole("button", { name: "Remove Test key" }).click();
  await again.getByRole("heading", { name: "Confirm that it is you" }).waitFor({ timeout: TIMEOUT_MS });
  await again.getByLabel("Recovery code").fill("0000-0000-0000-0000");
  await again.getByRole("button", { name: "Confirm with the code" }).click();
  await again.getByText("Nothing changed.", { exact: false }).waitFor({ timeout: TIMEOUT_MS });
  await again.getByRole("cell", { name: "Test key", exact: true }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "a removal in a new window needs a proof, and a wrong recovery code changes nothing");

  // 3. The next login asks for the passkey after the password.
  await logout(page, user);
  await page.getByText("You logged out.").waitFor({ timeout: TIMEOUT_MS });
  await password(page, user);
  await page.getByRole("button", { name: "Use my passkey" }).click();
  await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "after the password, the passkey logs in");

  // 4. A recovery code logs in one time.
  await logout(page, user);
  await password(page, user);
  await page.getByRole("button", { name: "I lost my passkey: use a recovery code" }).click();
  await answer(page, "Recovery code", codes[0].toUpperCase());
  await page.getByRole("button", { name: `Logged in as ${user}` }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "a recovery code logs in in place of the passkey");
  await logout(page, user);
  await password(page, user);
  await page.getByRole("button", { name: "I lost my passkey: use a recovery code" }).click();
  await answer(page, "Recovery code", codes[0]);
  await page.getByRole("alert").filter({ hasText: "The passkey or the recovery code was not accepted." }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "a used recovery code fails");

  // 5. Without the authenticator, the passkey cannot log in.
  await page.getByRole("button", { name: "Try again" }).click();
  await cdp.send("WebAuthn.disable");
  await password(page, user);
  check(true, "the password alone no longer logs in: the login waits for the passkey");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
  if (process.env.FARGLASS_SHOTS) await browser?.contexts()[0]?.pages()[0]?.screenshot({ path: join(process.env.FARGLASS_SHOTS, "passkey-failed.png") }).catch(() => {});
} finally {
  await browser?.close();
  for (const child of children) {
    child.kill("SIGTERM");
  }
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
