// launcher.mjs - end-to-end test of the apps of the host in the launcher (F031)
//
// Usage: npm --prefix spikes/e2e run launcher
//
// The script starts farglassd in dev_user mode and the agent with a Wayland
// socket and a desktop entry in a temporary directory. The entry starts
// gnome-calculator with no D-Bus, thus it does not reach a calculator of a
// local session. The launcher must list the entry with the icon of the
// calculator, and a click must start the app in its own systemd --user scope.
// Its window must show with the icon of the app on the window and in the panel.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test if gnome-calculator is not installed.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const CALCULATOR = "/usr/bin/gnome-calculator";
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let failed = false;
let unit;

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

try {
  if (!existsSync(CALCULATOR)) {
    console.log("SKIP: gnome-calculator is not installed");
    process.exit(0);
  }
  buildAll();
  const socket = join(state, "agent.sock");
  const wayland = join(state, "wayland-0");
  const data = join(state, "share");
  mkdirSync(join(data, "applications"), { recursive: true });
  writeFileSync(
    join(data, "applications", "farglass-e2e-calculator.desktop"),
    [
      "[Desktop Entry]",
      "Type=Application",
      "Name=Rechner e2e",
      "Name[de]=Rechner e2e",
      "Comment=The calculator of the launcher test",
      "Icon=org.gnome.Calculator",
      "Exec=env DBUS_SESSION_BUS_ADDRESS=disabled: GDK_BACKEND=wayland gnome-calculator",
      "StartupWMClass=org.gnome.Calculator",
      "",
    ].join("\n"),
  );
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start(
    "farglass-session",
    "target/debug/farglass-session",
    [],
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, XDG_DATA_HOME: data, XDG_DATA_DIRS: "/usr/share", LANG: "de_AT.UTF-8" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1400, height: 900 } })).newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  // 1. The launcher lists the desktop entry with its icon.
  await page.getByRole("button", { name: "Apps" }).click();
  await page.locator(".launcher-search").fill("rechner");
  const entry = page.locator('#launcher [role=option][data-host="true"]').filter({ hasText: "Rechner e2e" });
  await entry.waitFor({ timeout: TIMEOUT_MS });
  check(true, "the launcher lists the desktop entry by its German name");
  await page.waitForFunction(() => document.querySelector('#launcher [data-host="true"] img')?.getAttribute("src")?.startsWith("blob:"), null, { timeout: TIMEOUT_MS });
  check(true, "the entry shows the icon of the hicolor theme");
  const all = await page.locator("#launcher [role=option]").count();
  await page.locator(".launcher-search").fill("");
  check((await page.locator("#launcher [role=option]").count()) > all, "the launcher lists the apps of /usr/share/applications too");
  await page.locator(".launcher-search").fill("rechner");

  // 2. Enter starts it: the launcher closes, and the window opens.
  await page.locator(".launcher-search").press("Enter");
  await page.locator("#launcher [role=option]").first().waitFor({ state: "detached", timeout: TIMEOUT_MS });
  const window = page.locator('.wm-window[aria-label="Calculator"], .wm-window[aria-label="Rechner"]');
  await window.waitFor({ timeout: 60_000 });
  check(true, `the app window shows: ${await window.getAttribute("aria-label")}`);
  // The first login opens a Settings window too: the check names the window of the app.
  await page.waitForFunction(
    () => document.querySelector('.wm-window[aria-label="Calculator"] .wm-icon, .wm-window[aria-label="Rechner"] .wm-icon')?.getAttribute("src")?.startsWith("blob:"),
    null,
    { timeout: TIMEOUT_MS },
  );
  check(true, "the window shows the icon of its app");
  const task = page.locator(".panel-tasks .wm-task").filter({ hasText: /Calculator|Rechner/ });
  check((await task.locator("img").getAttribute("src"))?.startsWith("blob:"), "the panel task shows the icon of its app");

  // 3. The app is in its own scope of the user manager.
  const units = execFileSync("systemctl", ["--user", "list-units", "--type=scope", "--plain", "--no-legend", "app-farglass-*"], { encoding: "utf8" });
  unit = units.split("\n").find((line) => line.includes("farglass\\x2de2e\\x2dcalculator"))?.split(/\s+/)[0];
  check(Boolean(unit), `the app runs in its own scope: ${unit}`);
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "launcher.png") });
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGKILL");
  if (unit) {
    try {
      execFileSync("systemctl", ["--user", "stop", unit]);
    } catch {
      // The scope ended with the app.
    }
  }
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
