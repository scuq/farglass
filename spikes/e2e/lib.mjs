// lib.mjs - helpers of the end-to-end tests

import { spawn, spawnSync } from "node:child_process";
import net from "node:net";
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

/**
 * Builds the three programs and the shell. Returns the directory of the programs.
 * `release` builds with optimizations, for measurements of speed.
 */
export function buildAll(release = false) {
  build("cargo", ["build", ...(release ? ["--release"] : []), "-p", "farglassd", "-p", "farglass-session", "-p", "farglass-login"]);
  build("npm", ["--prefix", "web", "run", "build"]);
  return release ? "target/release" : "target/debug";
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

/**
 * Starts a TCP relay on a free loopback port to `target` (host:port).
 * `cut()` closes each connection and refuses new ones until `restore()`.
 * Returns the port and the two functions.
 */
export async function relay(target) {
  const [host, port] = [target.slice(0, target.lastIndexOf(":")), Number(target.slice(target.lastIndexOf(":") + 1))];
  const sockets = new Set();
  let open = true;
  const server = net.createServer((client) => {
    if (!open) {
      client.destroy();
      return;
    }
    const upstream = net.connect(port, host);
    for (const socket of [client, upstream]) {
      sockets.add(socket);
      socket.on("close", () => sockets.delete(socket));
      socket.on("error", () => {});
    }
    client.pipe(upstream).pipe(client);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return {
    port: server.address().port,
    cut() {
      open = false;
      for (const socket of sockets) {
        socket.destroy();
      }
    },
    restore() {
      open = true;
    },
    close() {
      server.close();
    },
  };
}

/** Logs out through the session menu of the panel. */
export async function logout(page, user) {
  await page.getByRole("button", { name: `Logged in as ${user}` }).click();
  await page.getByRole("menuitem", { name: "Log out" }).click();
}

/** Opens an app through the launcher of the panel. */
export async function launch(page, title) {
  await page.getByRole("button", { name: "Apps" }).click();
  await page.getByRole("option", { name: new RegExp(`^${title}`) }).click();
}

/**
 * Brings a window to the front with its task button in the panel, as a user does. A window
 * of an app that the test starts opens in the background: the user did not ask for it (F041).
 */
export async function raise(page, window) {
  const title = await window.getAttribute("aria-label");
  const task = page.locator(".panel-tasks .wm-task").filter({ hasText: title }).first();
  if ((await task.getAttribute("aria-pressed")) !== "true") {
    await task.click();
  }
  await page.waitForTimeout(200);
}
