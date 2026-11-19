// session-browser.mjs - one login through the shell in Chromium, for spikes/session/check.sh
//
// Usage: node session-browser.mjs URL USER EXPECT
//
// EXPECT is "success", "logout", or "decline".
// With "success", the login must succeed with no confirm step, and the echo
// channel must work. With "logout", the login must succeed, and then the Log out
// button must give the login screen. With "decline", the login must show the confirm step for
// a local session. The script clicks Cancel, never the other button, and
// expects the message that the local session continues.
//
// Exit status: 0 if the result is as expected, 1 if not.

import { chromium } from "playwright";
import { echo, logout, TIMEOUT_MS } from "./lib.mjs";

const [url, user, expect] = process.argv.slice(2);
let failed = false;
const browser = await chromium.launch();

async function answer(page, label, text) {
  const field = page.getByLabel(label, { exact: true });
  await field.waitFor({ timeout: TIMEOUT_MS });
  await field.fill(text);
  await page.getByRole("button", { name: "Continue" }).click();
}

try {
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await page.goto(url);
  await answer(page, "Login", user);
  await answer(page, "Password", "secret");

  const confirm = page.getByRole("alertdialog");
  const desktop = page.getByText(`Logged in as ${user}`);
  await Promise.race([
    confirm.waitFor({ timeout: 60_000 }),
    desktop.waitFor({ timeout: 60_000 }),
  ]);

  if (expect === "decline") {
    if (!(await confirm.isVisible())) {
      throw new Error("The confirm step for the local session did not show.");
    }
    console.log(`PASS: the confirm step shows: ${(await confirm.textContent()).slice(0, 120)}...`);
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    await page.getByRole("alert").filter({ hasText: "The local session continues." }).waitFor({ timeout: TIMEOUT_MS });
    console.log("PASS: Cancel stops the login");
  } else {
    if (await confirm.isVisible()) {
      await page.getByRole("button", { name: "Cancel", exact: true }).click();
      throw new Error(`${user} has a local session. The test clicked Cancel.`);
    }
    console.log(`PASS: ${user} logged in`);
    const time = await echo(page, `hello from the web session of ${user}`);
    console.log(`PASS: the agent in the web session answers, round trip ${time} ms`);
    if (expect === "logout") {
      await logout(page, user);
      await page.getByText("You logged out.").waitFor({ timeout: TIMEOUT_MS });
      console.log(`PASS: ${user} logged out in the shell`);
    }
  }
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
} finally {
  await browser.close();
}
process.exit(failed ? 1 : 0);
