// settings.mjs - end-to-end test of the settings app in Chromium, on the real system bus
//
// Usage: npm --prefix spikes/e2e run settings
//
// The script starts farglassd in dev_user mode and the agent, and opens the
// settings app. Each page reads the real services of this host: hostname1,
// timedate1, NetworkManager, and login1. The script changes only the
// appearance. It opens the confirm step of Restart and clicks Cancel only.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, launch, start, TIMEOUT_MS } from "./lib.mjs";

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

const prettyName = /^PRETTY_NAME="?([^"\n]*)"?$/m.exec(readFileSync("/etc/os-release", "utf8"))?.[1];
const timezone = execFileSync("timedatectl", ["show", "-p", "Timezone", "--value"], { encoding: "utf8" }).trim();

try {
  buildAll();
  const socket = join(state, "agent.sock");
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 800 }, colorScheme: "light" });
  const cspViolations = [];
  context.on("console", (message) => {
    if (message.text().includes("Content Security Policy")) cspViolations.push(message.text());
  });
  const page = await context.newPage();
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  await launch(page, "Settings");
  const win = page.locator('.wm-window[aria-label="Settings"]');
  await win.locator(".settings-nav").waitFor({ timeout: TIMEOUT_MS });
  const go = async (label) => {
    await win.locator(".settings-nav").getByRole("button", { name: label, exact: true }).click();
    await win.locator(".settings-page h2").filter({ hasText: label }).waitFor({ timeout: TIMEOUT_MS });
  };

  // 1. Appearance: the dark theme applies at once and stays after a reload.
  await win.getByLabel("Dark").check();
  await page.waitForFunction(() => document.documentElement.dataset.theme === "dark", null, { timeout: TIMEOUT_MS });
  const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  check(background === "rgb(26, 26, 25)", "the dark theme applies at once");
  await win.getByLabel("Green").check();
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "settings-appearance.png") });
  await page.reload();
  await page.waitForFunction(() => document.documentElement.dataset.theme === "dark" && document.documentElement.dataset.accent === "green", null, {
    timeout: TIMEOUT_MS,
  });
  check(true, "the appearance comes back after a reload from the state store");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "settings-reload.png") });
  await win.locator(".settings-nav").waitFor({ timeout: TIMEOUT_MS });
  check((await win.locator('[aria-current="page"]').textContent()) === "Appearance", "the window opens again on its page");

  // 2. Date and time, and system: the values of this host.
  await go("Date and time");
  await win.locator('[data-field="host-time"]').waitFor({ timeout: TIMEOUT_MS });
  check((await win.locator("#settings-timezone").inputValue()) === timezone, `the timezone is the one of timedatectl: ${timezone}`);
  await go("System");
  await win.locator('[data-field="os"]').waitFor({ timeout: TIMEOUT_MS });
  check((await win.locator('[data-field="os"]').textContent()) === prettyName, `the operating system is the one of os-release: ${prettyName}`);

  // 3. Network and sessions.
  await go("Network");
  await win.locator(".settings-item").first().waitFor({ timeout: TIMEOUT_MS });
  const devices = await win.locator(".settings-item strong").allTextContents();
  check(devices.length > 0 && !devices.includes("lo"), `the network page shows the devices of NetworkManager: ${devices.join(", ")}`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "settings-network.png") });
  await go("Users and sessions");
  await win.locator("tr[data-session]").first().waitFor({ timeout: TIMEOUT_MS });
  const users = await win.locator("tr[data-session] td:first-child").allTextContents();
  check(users.some((name) => name.startsWith(userInfo().username)), "the sessions of this user show");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "settings-users.png") });

  // 4. Power: the confirm step shows, and Cancel stops the action.
  await go("Power");
  const restart = win.locator(".settings-page").getByRole("button", { name: "Restart" });
  // The button is available after the answer of CanReboot.
  const can = execFileSync("busctl", ["--system", "call", "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager", "CanReboot"], {
    encoding: "utf8",
  });
  if (/"(yes|challenge)"/.test(can)) {
    await page.waitForFunction(() => {
      const button = [...document.querySelectorAll(".settings-page button")].find((each) => each.textContent === "Restart");
      return button && !button.disabled;
    }, null, { timeout: TIMEOUT_MS });
    await restart.click();
    const dialog = page.locator("dialog.settings-confirm");
    await dialog.waitFor({ timeout: TIMEOUT_MS });
    check(await dialog.getByRole("button", { name: "Cancel" }).evaluate((el) => el === document.activeElement), "the confirm step of Restart has the focus on Cancel");
    if (SHOTS) await page.screenshot({ path: join(SHOTS, "settings-power.png") });
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await dialog.waitFor({ state: "detached", timeout: TIMEOUT_MS });
    check(true, "Cancel closes the confirm step");
  } else {
    console.log("INFO: logind does not permit a restart here, the confirm step is not tested");
  }

  // Back to the default appearance for the next tests of this state directory.
  await go("Appearance");
  await win.getByLabel("Same as this computer").check();
  await win.getByLabel("Blue").check();
  check(cspViolations.length === 0, "no Content Security Policy violation");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGTERM");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
