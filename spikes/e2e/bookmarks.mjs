// bookmarks.mjs - end-to-end test of the bookmarks app (F100)
//
// Usage: npm --prefix spikes/e2e run bookmarks
//
// The script starts farglassd in dev_user mode and the agent with a temporary
// home directory. The home has a Firefox profiles.ini with the profile "work"
// and a Chromium Local State with the profile "Work" in "Profile 1". The
// desktop entries firefox and chromium start a script that writes its
// arguments to a file, thus no real browser starts.
//
// The test opens the bookmarks of a file with a comment, opens each one, adds
// one in the form, turns on the launcher search, reads a change by hand, and
// deletes a bookmark. The file must keep its comment.
//
// Exit status: 0 if all steps pass, 1 if a step fails.

import { chmodSync, existsSync, mkdirSync, realpathSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, launch, raise, start, TIMEOUT_MS } from "./lib.mjs";

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

/** Waits until `read()` returns a value that is not undefined. */
async function until(read, text) {
  const end = Date.now() + TIMEOUT_MS;
  for (;;) {
    const value = read();
    if (value !== undefined) return value;
    if (Date.now() > end) throw new Error(`timeout: ${text}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

const argsOf = (name) => {
  const path = join(state, `args-${name}`);
  return existsSync(path) ? readFileSync(path, "utf8").trim().split("\n") : undefined;
};

try {
  buildAll();
  const socket = join(state, "agent.sock");
  const home = join(state, "home");
  const config = join(home, ".config");
  const data = join(state, "share");
  mkdirSync(join(data, "applications"), { recursive: true });
  mkdirSync(join(home, ".mozilla/firefox"), { recursive: true });
  mkdirSync(join(config, "chromium"), { recursive: true });
  mkdirSync(join(config, "farglass"), { recursive: true });
  writeFileSync(join(home, ".mozilla/firefox/profiles.ini"), "[Profile0]\nName=default-release\nPath=a.default-release\n\n[Profile1]\nName=work\nPath=b.work\n");
  writeFileSync(join(config, "chromium/Local State"), JSON.stringify({ profile: { info_cache: { Default: { name: "Person 1" }, "Profile 1": { name: "Work" } } } }));
  for (const [id, name] of [
    ["firefox", "Firefox e2e"],
    ["chromium", "Chromium e2e"],
  ]) {
    const script = join(state, `fake-${id}`);
    writeFileSync(script, `#!/bin/sh\nprintf '%s\\n' "$@" > '${join(state, `args-${id}`)}'\n`);
    chmodSync(script, 0o755);
    writeFileSync(join(data, "applications", `${id}.desktop`), ["[Desktop Entry]", "Type=Application", `Name=${name}`, `Exec=${script} %u`, ""].join("\n"));
  }
  mkdirSync(join(home, "proj"), { recursive: true });
  const file = join(config, "farglass/bookmarks.toml");
  writeFileSync(
    file,
    [
      "# my bookmarks, by hand",
      "[[bookmark]]",
      'name = "gh" # the code',
      'url = "https://github.com/"',
      'browser = "firefox"',
      'profile = "work"',
      "private = true",
      "",
      "[[bookmark]]",
      'name = "docs"',
      'url = "https://docs.example/"',
      'browser = "client"',
      "",
      "[[bookmark]]",
      'name = "bravely"',
      'url = "https://brave.example/"',
      'browser = "brave"',
      "",
      "[[bookmark]]",
      'name = "proj"',
      'url = "~/proj"',
      'browser = "terminal"',
      "",
      "[[bookmark]]",
      'name = "fw"',
      'url = "ssh://nobody@127.0.0.1:1"',
      'browser = "terminal"',
      "",
      "[[bookmark]]",
      'name = "gone"',
      'url = "/no/such/folder/farglass"',
      'browser = "terminal"',
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
    { FARGLASS_AGENT_SOCKET: socket, HOME: home, XDG_CONFIG_HOME: config, XDG_DATA_HOME: data, XDG_DATA_DIRS: join(state, "none"), FARGLASS_TERMINAL_SCOPE: "0" },
    /connected to farglassd/,
  );
  children.push(agent.child);
  await agent.match;

  browser = await chromium.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1400, height: 900 } });
  // docs.example does not resolve: the test answers it, thus the new tab has its URL.
  await context.route("https://docs.example/**", (route) => route.fulfill({ contentType: "text/html", body: "<title>docs</title>" }));
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  // 1. The app lists the bookmarks of the file, with the problem of a browser that is not installed.
  await launch(page, "Bookmarks");
  const app = page.locator('.wm-window[aria-label="Bookmarks"]');
  const row = (name) => app.locator(".bookmarks-row").filter({ has: page.locator(".bookmarks-name", { hasText: new RegExp(`^${name}$`) }) });
  await row("gh").waitFor({ timeout: TIMEOUT_MS });
  check((await row("gh").locator(".bookmarks-line").first().textContent()) === "Firefox e2e · work · private · github.com", "a bookmark shows its browser, profile, private window, and host");
  check((await row("bravely").locator(".bookmarks-problem").textContent()) === "brave is not installed on this host.", "a bookmark of a browser that is not installed shows the problem");
  check(await row("bravely").locator(".bookmarks-open").isDisabled(), "that bookmark does not open");

  // 2. A click opens the bookmark in Firefox with the profile and a private window.
  await row("gh").locator(".bookmarks-open").click();
  const firefox = await until(() => argsOf("firefox"), "firefox starts");
  check(JSON.stringify(firefox) === JSON.stringify(["-P", "work", "--private-window", "https://github.com/"]), `firefox gets ${firefox.join(" ")}`);
  await app.locator(".settings-outcome").filter({ hasText: "gh opened in Firefox e2e." }).waitFor({ timeout: TIMEOUT_MS });
  check(true, "the app tells that the bookmark opened");

  // 3. A bookmark of the client browser opens a new tab here.
  const [tab] = await Promise.all([context.waitForEvent("page", { timeout: TIMEOUT_MS }), row("docs").locator(".bookmarks-open").click()]);
  await tab.waitForLoadState("commit").catch(() => {});
  check(tab.url() === "https://docs.example/", `the client bookmark opens a new tab: ${tab.url()}`);
  check(await tab.evaluate(() => window.opener === null), "the new tab has no handle to the shell");
  await tab.close();

  // 3b. A terminal bookmark of a folder opens a terminal window with the shell in that folder (F110).
  const terminalText = (name) => page.locator(`.wm-window[aria-label="${name}"] .term-app`).evaluate((el) => el.farglassTerminal.text());
  const terminalShows = async (name, pattern) => {
    const end = Date.now() + TIMEOUT_MS;
    for (;;) {
      const text = await terminalText(name).catch(() => "");
      if (pattern.test(text)) return text;
      if (Date.now() > end) throw new Error(`The terminal ${name} does not show ${pattern}:\n${text}`);
      await page.waitForTimeout(100);
    }
  };
  check((await row("proj").locator(".bookmarks-line").first().textContent()) === "Terminal · ~/proj", "a folder bookmark shows Terminal and the folder");
  await row("proj").locator(".bookmarks-open").click();
  const proj = page.locator('.wm-window[aria-label="proj"] .term-app[data-status="running"]');
  await proj.waitFor({ timeout: TIMEOUT_MS });
  await proj.click();
  await page.keyboard.type("echo at-$(pwd -P)");
  await page.keyboard.press("Enter");
  await terminalShows("proj", new RegExp(`at-${realpathSync(join(home, "proj"))}`));
  check(true, "the folder bookmark opens a terminal in that folder");

  // 3c. An ssh bookmark starts ssh with the user, the host, and the port of the URL.
  // The new terminal window is on top: the Bookmarks window comes back to the front.
  await raise(page, app);
  check((await row("fw").locator(".bookmarks-line").first().textContent()) === "Terminal · ssh nobody@127.0.0.1:1", "an ssh bookmark shows the destination");
  await row("fw").locator(".bookmarks-open").click();
  await terminalShows("fw", /port 1: Connection refused[\s\S]*ssh stopped with exit code 255\. Press Enter to connect again\./);
  check(true, "the ssh bookmark starts ssh to 127.0.0.1 port 1, and tells that ssh stopped");
  await raise(page, app);
  check((await row("gone").locator(".bookmarks-problem").textContent()).includes("does not exist on the host"), "a folder that does not exist shows the problem");

  // 4. The form adds a Chromium bookmark with the profile Work. The file keeps its comments.
  await app.getByRole("button", { name: "Add" }).click();
  await app.locator("#bookmark-name").fill("mail");
  await app.locator("#bookmark-url").fill("https://mail.example/inbox");
  await app.locator("#bookmark-browser").selectOption("chromium");
  check(JSON.stringify(await app.locator("#bookmark-profile option").allTextContents()) === JSON.stringify(["Default profile", "Person 1", "Work"]), "the profile list has the profiles of Chromium");
  await app.locator("#bookmark-profile").selectOption("Work");
  await app.getByRole("button", { name: "Save" }).click();
  await row("mail").waitFor({ timeout: TIMEOUT_MS });
  const text = readFileSync(file, "utf8");
  check(text.startsWith("# my bookmarks, by hand") && text.includes('name = "gh" # the code'), "the save keeps the comments of the file");
  check(text.includes('name = "mail"') && text.includes('profile = "Work"') && text.includes('browser = "chromium"'), "the file has the new bookmark");
  await row("mail").locator(".bookmarks-open").click();
  const chromiumArgs = await until(() => argsOf("chromium"), "chromium starts");
  check(JSON.stringify(chromiumArgs) === JSON.stringify(["--profile-directory=Profile 1", "https://mail.example/inbox"]), `chromium gets ${chromiumArgs.join(" ")}`);

  // 5. The launcher finds the bookmarks only after the option is on.
  const search = async (query) => {
    await page.getByRole("button", { name: "Apps" }).click();
    await page.locator(".launcher-search").fill(query);
    await page.waitForTimeout(300);
    const titles = await page.locator("#launcher [role=option] .launcher-title").allTextContents();
    // The first Escape clears the search field, the second closes the launcher.
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await page.locator("#launcher [role=option]").first().waitFor({ state: "hidden", timeout: TIMEOUT_MS });
    return titles;
  };
  check(!(await search("mail")).includes("mail"), "by default the launcher does not show the bookmarks");
  // The check box follows the file: it shows the change after the save.
  await app.getByLabel("Show in launcher search").click();
  await app.locator(".settings-outcome").filter({ hasText: "The launcher shows the bookmarks." }).waitFor({ timeout: TIMEOUT_MS });
  check(readFileSync(file, "utf8").includes("launcher = true"), "the option is in the file");
  check((await search("mail")).includes("mail"), "with the option, the launcher finds the bookmark by its short name");

  // 6. A change by hand shows after Reload.
  writeFileSync(file, readFileSync(file, "utf8").replace('name = "docs"', 'name = "handbook"'));
  await app.getByRole("button", { name: "Reload" }).click();
  await row("handbook").waitFor({ timeout: TIMEOUT_MS });
  check(true, "a change by hand shows in the app");

  // 7. Edit shows the buttons of each row (F106). Delete asks first, then removes the bookmark from the file.
  check((await app.getByRole("button", { name: "Delete bravely" }).count()) === 0, "the rows have no Delete button before Edit");
  await app.getByRole("button", { name: "Edit", exact: true }).click();
  await row("bravely").getByRole("button", { name: "Delete bravely" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await row("bravely").waitFor({ state: "detached", timeout: TIMEOUT_MS });
  check(!readFileSync(file, "utf8").includes("bravely"), "the deleted bookmark is not in the file");
  if (SHOTS) await page.screenshot({ path: join(SHOTS, "bookmarks.png") });
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
