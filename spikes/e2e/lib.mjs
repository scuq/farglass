// lib.mjs - helpers of the end-to-end tests

import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";

export const root = resolve(import.meta.dirname, "../..");
export const TIMEOUT_MS = 15_000;

/** Runs a build command and throws if it fails. */
export function build(command, args) {
  console.log(`$ ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit status ${result.status}.`);
  }
}

/** Builds the three programs and the shell. */
export function buildAll() {
  build("cargo", ["build", "-p", "farglassd", "-p", "farglass-session", "-p", "farglass-login"]);
  build("npm", ["--prefix", "web", "run", "build"]);
}

/** Starts a program and returns it with the first match of `pattern` in its log. */
export function start(name, command, args, env, pattern) {
  const child = spawn(command, args, {
    cwd: root,
    env: { ...process.env, RUST_LOG: "info", NO_COLOR: "1", ...env },
    stdio: ["ignore", "inherit", "pipe"],
  });
  let log = "";
  const match = new Promise((resolveMatch, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`${name} did not log ${pattern} in ${TIMEOUT_MS} ms:\n${log}`)),
      TIMEOUT_MS,
    );
    child.stderr.on("data", (chunk) => {
      log += chunk;
      const found = log.match(pattern);
      if (found) {
        clearTimeout(timer);
        resolveMatch(found);
      }
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`${name} stopped with exit status ${code}:\n${log}`));
    });
  });
  return { child, match, log: () => log };
}

/** Sends `text` on the echo panel of `page` and returns the round-trip time from the log line. */
export async function echo(page, text) {
  await page.getByRole("status").filter({ hasText: "Echo channel open" }).waitFor({ timeout: TIMEOUT_MS });
  await page.getByLabel("Message").fill(text);
  await page.getByRole("button", { name: "Send" }).click();
  const line = page.locator('.echo-log li[data-kind="echo"]').filter({ hasText: text });
  await line.waitFor({ timeout: TIMEOUT_MS });
  return (await line.textContent()).match(/\(([\d.]+) ms\)/)?.[1];
}
