// netcheck.mjs - end-to-end test of the Network check app (F111)
//
// Usage: npm --prefix spikes/e2e run netcheck
//
// The script starts farglassd in dev_user mode and the agent with a temporary
// home directory, and a TCP listener on 127.0.0.1. The test opens the app,
// waits for the internet and DNS checks, adds a TCP check to the listener, a
// TCP check to a closed port, and an ICMP check of 127.0.0.1, runs one check
// again, deletes a check, and opens the app from Settings > Network.
//
// The internet checks need no internet: the test only waits for a result.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, launch, raise, start, TIMEOUT_MS } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let browser;
let listener;
let failed = false;

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

/** Returns a free TCP port of 127.0.0.1 with no listener. */
async function closedPort() {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  await new Promise((resolve) => server.close(resolve));
  return port;
}

try {
  buildAll();
  const socket = join(state, "agent.sock");
  const home = join(state, "home");
  const config = join(home, ".config");
  mkdirSync(config, { recursive: true });
  listener = createServer((connection) => connection.end());
  await new Promise((resolve) => listener.listen(0, "127.0.0.1", resolve));
  const open = listener.address().port;
  const closed = await closedPort();

  const daemon = start(
    "farglassd",
    "target/debug/farglassd",
    ["--listen", "127.0.0.1:0", "--web-root", "web/dist", "--state-dir", state, "--agent-socket", socket, "--dev-user", String(process.getuid())],
    {},
    /listening on (https:\/\/\S+)/,
  );
  children.push(daemon.child);
  const [, url] = await daemon.match;
  const agent = start("farglass-session", "target/debug/farglass-session", [], { FARGLASS_AGENT_SOCKET: socket, HOME: home, XDG_CONFIG_HOME: config, FARGLASS_TERMINAL_SCOPE: "0" }, /connected to farglassd/);
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1400, height: 900 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  // 1. The app runs the internet and DNS checks when it opens. Each one ends with a result.
  await launch(page, "Network check");
  const app = page.locator('.wm-window[aria-label="Network check"]').first();
  const row = (id) => app.locator(`.netcheck-row[data-id="${id}"]`);
  const done = (id) => row(id).locator('.netcheck-status:not([data-status="checking"]):not([data-status="not-checked"])').waitFor({ timeout: TIMEOUT_MS });
  for (const id of ["internet:1.1.1.1", "internet:google.com", "dns"]) {
    await done(id);
    check(true, `${id} has a result: ${(await row(id).locator(".netcheck-status").textContent()).trim()}`);
  }
  const internet = (await app.locator(".netcheck-summary dd").first().textContent()).trim();
  check(/Available|Failed|No answer/.test(internet), `the summary shows the internet: ${internet}`);
  check((await app.locator(".netcheck-empty").textContent()).includes("No checks yet"), "with no file, the list of the user is empty");

  // 2. Add checks: an open port, a closed port, and ICMP to 127.0.0.1. Each one runs after the save.
  const add = async (name, kind, host, port) => {
    await app.getByRole("button", { name: "Add check" }).click();
    await app.getByLabel("Name", { exact: true }).fill(name);
    await app.getByLabel("Kind", { exact: true }).selectOption(kind);
    await app.getByLabel("Host", { exact: true }).fill(host);
    if (port !== undefined) await app.getByLabel("Port", { exact: true }).fill(String(port));
    await app.getByRole("button", { name: "Save" }).click();
    await app.locator(".settings-outcome").filter({ hasText: `${name} is saved.` }).waitFor({ timeout: TIMEOUT_MS });
  };
  await add("listener", "tcp", "127.0.0.1", open);
  await add("nothing", "tcp", "127.0.0.1", closed);
  await add("loopback", "icmp", "127.0.0.1");
  const file = readFileSync(join(config, "farglass/netchecks.toml"), "utf8");
  check(file.startsWith("# The connection checks of farglass.") && file.includes(`port = ${open}`), "the checks are in the file of the user");
  for (const id of ["user:0", "user:1", "user:2"]) await done(id);
  check((await row("user:0").locator(".netcheck-status").textContent()).includes("Available"), "the open port is available");
  check((await row("user:0").locator(".netcheck-ms").textContent()).endsWith(" ms"), "a passed check shows its time in ms");
  check((await row("user:1").locator(".netcheck-status").textContent()).includes("Failed"), "the closed port failed");
  check((await row("user:1").locator(".netcheck-detail").textContent()).startsWith("Refused"), "the closed port tells that the host refused");
  check((await row("user:2").locator(".netcheck-status").textContent()).includes("Available"), "ICMP to 127.0.0.1 is available");
  check((await app.locator('[data-field="checks-count"]').textContent()).includes("2 of 3 available"), "the summary counts the checks of the user");

  // 3. Check runs one check again.
  await row("user:0").getByRole("button", { name: "Check listener again" }).click();
  await done("user:0");
  check(true, "one check runs again");

  // 4. Edit, then Delete asks first and removes the check from the file.
  await app.getByRole("button", { name: "Edit", exact: true }).click();
  await row("user:1").getByRole("button", { name: "Delete nothing" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await app.locator(".settings-outcome").filter({ hasText: "nothing is deleted." }).waitFor({ timeout: TIMEOUT_MS });
  check(!readFileSync(join(config, "farglass/netchecks.toml"), "utf8").includes("nothing"), "the deleted check is not in the file");
  await app.getByRole("button", { name: "Done", exact: true }).click();
  if (SHOTS) await app.screenshot({ path: join(SHOTS, "netcheck.png") });

  // 5. Settings > Network opens the app.
  await launch(page, "Settings");
  const settings = page.locator('.wm-window[aria-label="Settings"]').first();
  await settings.locator(".settings-nav").waitFor({ timeout: TIMEOUT_MS });
  await raise(page, settings);
  await settings.locator(".settings-nav").getByRole("button", { name: "Network", exact: true }).click();
  const button = settings.getByRole("button", { name: "Check connections" });
  if (await button.waitFor({ timeout: TIMEOUT_MS }).then(() => true, () => false)) {
    await button.click();
    await page.locator('.wm-window[aria-label="Network check 2"]').waitFor({ timeout: TIMEOUT_MS });
    check(true, "Settings > Network opens a Network check window");
  } else {
    console.log("SKIP: NetworkManager does not run on this host, thus the network page has no button");
  }
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  listener?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
