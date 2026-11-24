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
  // The first login opens the Settings window. A second window would cover the first one.
  const win = page.locator('.wm-window[aria-label="Settings"]');
  await win.waitFor({ timeout: 5000 }).catch(() => launch(page, "Settings"));
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

  // 5. The filter shows the matching cards of each page, and Escape clears it.
  const filter = win.getByLabel("Filter settings");
  const cards = () => win.locator("[data-section]").evaluateAll((all) => all.map((card) => card.dataset.section));
  await filter.fill("timezone");
  await win.locator("#settings-timezone").waitFor({ timeout: TIMEOUT_MS });
  check((await cards()).join() === "datetime-host", "the filter shows only the card with the timezone");
  check((await win.locator('[aria-current="page"]').count()) === 0, "no page is the open page while the filter has text");
  await filter.fill("host");
  await win.locator('[data-section="system-names"]').waitFor({ timeout: TIMEOUT_MS });
  if (SHOTS) await win.screenshot({ path: join(SHOTS, "settings-filter.png") });
  await filter.fill("no such setting");
  await win.locator(".settings-empty").waitFor({ timeout: TIMEOUT_MS });
  check((await cards()).length === 0, "a filter with no match shows no card and tells it");
  await filter.press("Escape");
  await win.locator('[aria-current="page"]').waitFor({ timeout: TIMEOUT_MS });
  check((await filter.inputValue()) === "" && (await win.count()) === 1, "Escape clears the filter and the window stays open");

  // 6. The text inputs of a page have the same width, also the buttons.
  await go("System");
  await win.locator("#settings-hostname").waitFor({ timeout: TIMEOUT_MS });
  const widths = async (selector) => win.locator(selector).evaluateAll((all) => all.map((each) => Math.round(each.getBoundingClientRect().width)));
  const inputs = await widths('[data-section="system-names"] input');
  const buttons = await widths('[data-section="system-names"] button');
  check(inputs.length === 2 && inputs[0] === inputs[1], `the inputs for the hostname and the display name have the same width: ${inputs.join(", ")}`);
  check(buttons[0] === buttons[1], `their buttons have the same width: ${buttons.join(", ")}`);
  if (SHOTS) await win.screenshot({ path: join(SHOTS, "settings-system.png") });
  await go("Date and time");
  await win.locator('[data-field="host-time"]').waitFor({ timeout: TIMEOUT_MS });
  if (SHOTS) await win.screenshot({ path: join(SHOTS, "settings-datetime.png") });
  // The use of the host with a graph of its history. The second sample of the agent starts the line.
  await go("Resources");
  const hostCpu = win.locator('[data-field="host-cpu"]');
  await hostCpu.waitFor({ timeout: TIMEOUT_MS });
  check(/^\d+\.\d %$/.test(await hostCpu.textContent()), `the resource page shows the CPU of the host: ${await hostCpu.textContent()}`);
  await page.waitForFunction(() => [...document.querySelectorAll(".spark-plot .spark-line")].every((line) => line.getAttribute("d")?.startsWith("M")), null, { timeout: TIMEOUT_MS });
  const stroke = await win.locator(".spark-line").first().evaluate((line) => [getComputedStyle(line).stroke, getComputedStyle(document.documentElement).getPropertyValue("--focus")]);
  check((await win.locator(".spark-plot").count()) === 6 && stroke[0] !== "none", `each number has a graph in the accent color: ${stroke[0]}`);
  const plot = await win.locator(".spark-plot").first().boundingBox();
  await page.mouse.move(plot.x + plot.width - 2, plot.y + plot.height / 2);
  await win.locator(".spark-caption").first().filter({ hasText: /^(now|\d+ s ago): / }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "the pointer over a graph shows the value at that time");
  await page.mouse.move(plot.x - 40, plot.y - 40);
  if (SHOTS) await win.screenshot({ path: join(SHOTS, "settings-resources.png") });

  // 7. A narrow window: the pages go to the top, and the mark of this session stays in one piece.
  const frame = await win.boundingBox();
  const grip = await win.locator(".wm-edge-se").boundingBox();
  await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
  await page.mouse.down();
  await page.mouse.move(frame.x + 420, frame.y + 620, { steps: 8 });
  await page.mouse.up();
  await page.waitForFunction(() => document.querySelector('.wm-window[aria-label="Settings"]').getBoundingClientRect().width < 460, null, { timeout: TIMEOUT_MS });
  const side = await win.locator(".settings-side").boundingBox();
  const main = await win.locator(".settings-main").boundingBox();
  check(side.y + side.height <= main.y + 1 && Math.abs(side.width - main.width) < 2, "in a narrow window the filter and the pages are above the page");
  for (const [label, name] of [["System", "system"], ["Date and time", "datetime"], ["Resources", "resources"], ["Users and sessions", "users"]]) {
    await go(label);
    await win.locator("[data-section]").first().waitFor({ timeout: TIMEOUT_MS });
    const overflow = await win.locator(".settings-main").evaluate((el) => el.scrollWidth - el.clientWidth);
    check(overflow <= 0, `the page ${label} has no horizontal scroll in a narrow window`);
    if (SHOTS) await win.screenshot({ path: join(SHOTS, `settings-narrow-${name}.png`) });
  }
  await win.locator("tr[data-session]").first().waitFor({ timeout: TIMEOUT_MS });
  // A session of farglassd in dev_user mode is not a web session of logind, thus it has no mark.
  if ((await win.locator(".settings-badge").count()) > 0) {
    const lines = await win.locator(".settings-badge").first().evaluate((el) => el.getClientRects().length);
    check(lines === 1, "the mark This session is on one line in a narrow window");
  } else {
    console.log("INFO: no session has the mark This session here, its line break is not tested");
  }
  if (SHOTS) {
    await go("Appearance");
    // The page shows the default values until the state store answers.
    await win.locator('input[value="dark"]:checked').waitFor({ timeout: TIMEOUT_MS });
    await win.getByLabel("LCARS").check();
    await page.waitForFunction(() => document.documentElement.dataset.theme === "lcars", null, { timeout: TIMEOUT_MS });
    await win.getByLabel("Orange").check();
    await page.waitForFunction(() => document.documentElement.dataset.accent === "orange", null, { timeout: TIMEOUT_MS });
    for (const [label, name] of [["System", "system"], ["Date and time", "datetime"]]) {
      await go(label);
      await win.locator("[data-section] input").first().waitFor({ timeout: TIMEOUT_MS });
      await win.screenshot({ path: join(SHOTS, `settings-lcars-${name}.png`) });
    }
  }

  // Back to the default appearance for the next tests of this state directory.
  await go("Appearance");
  await win.getByRole("group", { name: "Theme" }).getByLabel("Same as this computer").check();
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
