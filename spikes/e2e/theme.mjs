// theme.mjs - end-to-end test of the theme of the apps of the host (F120, ADR-059)
//
// Usage: npm --prefix spikes/e2e run theme
//
// The script starts farglassd in dev_user mode and the agent with
// FARGLASS_FOLLOW_THEME=1, as in a web session. GSettings uses its keyfile
// backend in a temporary directory, thus the test changes no setting of the
// user. The shell starts with the LCARS theme: the agent must set the dark
// GSettings values. Light in Settings must give the light values. A SIGTERM
// to the agent, as at the end of the web session, must bring back the values
// before the test, and delete the file of the saved values.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test with no gsettings or no schema org.gnome.desktop.interface.

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, launch, start, TIMEOUT_MS } from "./lib.mjs";

const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let failed = false;

// The settings of the test: GSettings in a keyfile, and the saved values in a directory of the test.
const env = {
  ...process.env,
  GSETTINGS_BACKEND: "keyfile",
  XDG_CONFIG_HOME: join(state, "config"),
  XDG_STATE_HOME: join(state, "state"),
};
const saved = join(state, "state", "farglass", "theme-restore.json");

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

function gsettings(...args) {
  return execFileSync("gsettings", args, { env, encoding: "utf8" }).trim();
}

const get = (key) => gsettings("get", "org.gnome.desktop.interface", key);

/** Waits until the key `key` has the value `value`. */
async function until(key, value) {
  const end = Date.now() + TIMEOUT_MS;
  while (get(key) !== value) {
    if (Date.now() > end) throw new Error(`${key} is ${get(key)}, not ${value}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

try {
  try {
    gsettings("get", "org.gnome.desktop.interface", "color-scheme");
  } catch {
    console.log("SKIP: no gsettings, or no schema org.gnome.desktop.interface with color-scheme");
    process.exit(0);
  }
  gsettings("set", "org.gnome.desktop.interface", "color-scheme", "default");
  gsettings("set", "org.gnome.desktop.interface", "gtk-theme", "Adwaita");
  buildAll();
  const socket = join(state, "agent.sock");
  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", join(state, "farglassd"), "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start(
    "farglass-session",
    "target/debug/farglass-session",
    [],
    { ...env, FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: join(state, "wayland-0"), FARGLASS_FOLLOW_THEME: "1" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });
  await until("color-scheme", "'prefer-dark'");
  check(get("gtk-theme") === "'Adwaita-dark'", "the LCARS theme of the shell gives dark GTK apps: color-scheme prefer-dark, gtk-theme Adwaita-dark");
  check(existsSync(saved), "the agent saved the values of the user before its first change");

  const win = page.locator('.wm-window[aria-label="Settings"]');
  await win.waitFor({ timeout: 5000 }).catch(() => launch(page, "Settings"));
  await win.getByLabel("Light").check();
  await page.waitForFunction(() => document.documentElement.dataset.theme === "light", null, { timeout: TIMEOUT_MS });
  await until("color-scheme", "'prefer-light'");
  check(get("gtk-theme") === "'Adwaita'", "the light theme of the shell gives light GTK apps at once");

  // The end of the web session: systemd stops the agent with SIGTERM.
  const exited = new Promise((resolve) => agent.child.on("exit", resolve));
  agent.child.kill("SIGTERM");
  await exited;
  check(get("color-scheme") === "'default'" && get("gtk-theme") === "'Adwaita'", "at the end of the session, the values of the user come back");
  check(!existsSync(saved), "the agent deletes the file of the saved values");
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
