// youtube.mjs - spike of video redirection with a public video site (F039)
//
// Usage: FARGLASS_MEASURE=1 npm --prefix spikes/e2e run youtube
//
// The same chain as spikes/e2e/redirect.mjs, with a YouTube video in the host
// browser: FARGLASS_REDIRECT_URL, or a video of the Blender Foundation. It
// rejects the consent of Google, waits until the client plays the stream, and
// measures the CPU of the host browser and of the agent. The result is in
// ADR of F039. This script needs the network and is not in the test suite.

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, root, start, TIMEOUT_MS, raise } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const MEASURE = process.env.FARGLASS_MEASURE === "1";
const EXTENSION = join(root, "extension", "redirect");
const state = mkdtempSync(join(tmpdir(), "farglass-spike-"));
const children = [];
let host;
let browser;
let server;
let failed = false;
let agentLog = () => "";

function check(condition, text) {
  if (!condition) {
    throw new Error(text);
  }
  console.log(`PASS: ${text}`);
}

/** Returns the CPU seconds of a process and of each process under it. */
function cpuOfTree(pid) {
  const tick = 100;
  const all = readdirSync("/proc").filter((name) => /^\d+$/.test(name));
  const parent = new Map();
  const cpu = new Map();
  for (const name of all) {
    try {
      const stat = readFileSync(`/proc/${name}/stat`, "utf8");
      const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
      parent.set(Number(name), Number(fields[1]));
      cpu.set(Number(name), (Number(fields[11]) + Number(fields[12])) / tick);
    } catch {
      // The process ended.
    }
  }
  let total = 0;
  for (const [process, seconds] of cpu) {
    for (let p = process; p > 1; p = parent.get(p) ?? 0) {
      if (p === pid) {
        total += seconds;
        break;
      }
    }
  }
  return total;
}

try {
  const pageUrl = process.env.FARGLASS_REDIRECT_URL ?? "https://www.youtube.com/watch?v=aqz-KE-bpKQ";
  buildAll();
  const socket = join(state, "agent.sock");
  const wayland = join(state, "wayland-0");
  const redirectSocket = join(state, "redirect.sock");
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
    { FARGLASS_AGENT_SOCKET: socket, FARGLASS_WAYLAND_SOCKET: wayland, FARGLASS_REDIRECT_SOCKET: redirectSocket },
    /connected to farglassd/,
  );
  children.push(agent.child);
  agentLog = agent.log;
  await agent.match;

  // The client: the shell.
  browser = await chromium.launch();
  const shell = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: process.env.FARGLASS_HOST_SIZE ? { width: 1900, height: 1100 } : { width: 1400, height: 900 } })).newPage();
  const errors = [];
  shell.on("pageerror", (error) => errors.push(error.message));
  if (process.env.FARGLASS_DEBUG) shell.on("console", (message) => console.log(`SHELL ${message.type()}: ${message.text()}`));
  await shell.goto(url);
  await shell.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

  // The host browser: a Chromium on the compositor with the extension and its native messaging host.
  const profile = join(state, "host-profile");
  mkdirSync(join(profile, "NativeMessagingHosts"), { recursive: true });
  const manifest = JSON.parse(readFileSync(join(EXTENSION, "manifest.json"), "utf8"));
  const id = execFileSync("python3", [
    "-c",
    "import base64,hashlib,sys; d=hashlib.sha256(base64.b64decode(sys.argv[1])).hexdigest()[:32]; print(''.join(chr(97+int(c,16)) for c in d))",
    manifest.key,
  ])
    .toString()
    .trim();
  writeFileSync(
    join(profile, "NativeMessagingHosts", "org.farglass.redirect.json"),
    JSON.stringify({ name: "org.farglass.redirect", description: "test", path: join(root, "target/debug/farglass-session"), type: "stdio", allowed_origins: [`chrome-extension://${id}/`] }),
  );
  host = await chromium.launchPersistentContext(profile, {
    headless: false,
    viewport: null,
    locale: "en-US",
    args: ["--ozone-platform=wayland", `--disable-extensions-except=${EXTENSION}`, `--load-extension=${EXTENSION}`, process.env.FARGLASS_HOST_SIZE ? `--window-size=${process.env.FARGLASS_HOST_SIZE}` : "--window-size=900,700", "--autoplay-policy=no-user-gesture-required"],
    env: { ...process.env, WAYLAND_DISPLAY: wayland, DISPLAY: "", FARGLASS_REDIRECT_SOCKET: redirectSocket },
  });
  const player = host.pages()[0] ?? (await host.newPage());
  // The choice "reject all" of the consent dialog of Google, as a cookie: the dialog pauses the video.
  await host.addCookies(
    [".youtube.com", ".google.com"].flatMap((domain) => [
      { name: "SOCS", value: "CAESEwgDEgk0ODE3Nzk3MjQaAmVuIAEaBgiA_LyaBg", domain, path: "/", secure: true },
      { name: "CONSENT", value: "PENDING+987", domain, path: "/", secure: true },
    ]),
  );
  await player.goto(pageUrl, { waitUntil: "domcontentloaded" });
  // The consent page of Google in the EU: reject all, which plays the video too.
  const reject = player.getByRole("button", { name: /Reject all/ }).first();
  if (await reject.isVisible({ timeout: 8000 }).catch(() => false)) await reject.click();
  // Autoplay does not always start in a new profile: the spike starts the video itself.
  await player.waitForFunction(() => document.querySelector("video")?.readyState >= 2, null, { timeout: 60_000 });
  await player.evaluate(() => document.querySelector("video").play().catch(() => {}));
  await player.waitForFunction(() => [...document.querySelectorAll("video")].some((v) => v.currentTime > 1 && !v.paused), null, { timeout: 60_000 });
  const codecs = await player.evaluate(() => [...document.querySelectorAll("video")].map((v) => ({ src: v.currentSrc.slice(0, 30), w: v.videoWidth, h: v.videoHeight })));
  console.log("HOST VIDEOS", JSON.stringify(codecs));
  check(true, "the host plays the YouTube video");

  // 1. The client plays the same stream in a video element in the window of the host browser.
  const window = shell.locator('.wm-window[aria-label*="YouTube"]');
  await window.waitFor({ timeout: 60_000 });
  await raise(shell, window);
  const video = window.locator("video[data-redirect]");
  await video.waitFor({ timeout: 30_000 });
  await shell.waitForFunction(() => {
    const v = document.querySelector(".wm-window video[data-redirect]");
    return v && v.readyState >= 2 && !v.paused && v.currentTime > 0.5;
  }, null, { timeout: 30_000 });
  check(/a host browser connected for video redirection/.test(agent.log()), "the native messaging host connects to the agent");
  const t0 = await video.evaluate((v) => v.currentTime);
  await shell.waitForTimeout(1000);
  const t1 = await video.evaluate((v) => v.currentTime);
  check(t1 - t0 > 0.6, `the client decodes and plays the stream (${(t1 - t0).toFixed(2)} s in 1 s)`);

  if (SHOTS) await shell.screenshot({ path: join(SHOTS, "youtube.png") });
  // The theatre mode and back (key t): the client must show the video in each layout.
  const sample = async (label) => {
    await shell.waitForTimeout(3000);
    const box = await shell.locator("video[data-redirect]").boundingBox().catch(() => null);
    if (!box) return console.log(`SAMPLE ${label}: no client video`);
    const shot = await shell.screenshot({ clip: { x: box.x + box.width * 0.3, y: box.y + box.height * 0.3, width: box.width * 0.4, height: box.height * 0.3 } });
    if (SHOTS) {
      writeFileSync(join(SHOTS, `yt-${label}.png`), shot);
      await shell.screenshot({ path: join(SHOTS, `yt-full-${label}.png`) });
    }
    const level = await shell.evaluate(async (b64) => {
      const image = new Image();
      image.src = `data:image/png;base64,${b64}`;
      await image.decode();
      const canvas = new OffscreenCanvas(image.width, image.height);
      const context = canvas.getContext("2d");
      context.drawImage(image, 0, 0);
      const data = context.getImageData(0, 0, image.width, image.height).data;
      let sum = 0;
      for (let i = 0; i < data.length; i += 4) sum += data[i] + data[i + 1] + data[i + 2];
      return sum / (data.length / 4);
    }, shot.toString("base64"));
    console.log(`SAMPLE ${label}: brightness ${level.toFixed(0)} box ${Math.round(box.width)}x${Math.round(box.height)}`);
    console.log(`DIAG ${label}: ${JSON.stringify(await shell.evaluate(() => globalThis.farglassRedirect?.()))}`);
    // The client video alone, with the canvas hidden: is the video bright?
    const own = await shell.evaluate(() => {
      const video = document.querySelector("video[data-redirect]");
      const canvas = document.createElement("canvas");
      canvas.width = 64;
      canvas.height = 36;
      const context = canvas.getContext("2d");
      context.drawImage(video, 0, 0, 64, 36);
      const data = context.getImageData(0, 0, 64, 36).data;
      let sum = 0;
      for (let i = 0; i < data.length; i += 4) sum += data[i] + data[i + 1] + data[i + 2];
      return sum / (data.length / 4);
    });
    console.log(`VIDEO ${label}: own brightness ${own.toFixed(0)}`);
  };
  if (process.env.FARGLASS_THEATRE) {
    await sample("normal");
    await player.keyboard.press("t");
    await sample("theatre");
    await player.keyboard.press("t");
    await sample("back");
    await shell.waitForTimeout(4000);
    await sample("later");
  }
  const mode = await player.evaluate(() => [...document.querySelectorAll("video")].map((v) => `${v.style.getPropertyValue("visibility") || "-"} ${v.style.getPropertyValue("filter") || "-"}`));
  console.log("HOST DRAW", JSON.stringify(mode));
  const chain = await player.evaluate(() => {
    const out = [];
    for (let e = document.querySelector("video").parentElement; e && out.length < 8; e = e.parentElement) out.push(`${e.tagName.toLowerCase()}#${e.id}.${String(e.className).split(" ")[0]} ${getComputedStyle(e).backgroundColor}`);
    return out;
  });
  console.log("HOST BACKGROUNDS", JSON.stringify(chain));
  const mimes = await shell.evaluate(() => [...document.querySelectorAll("video[data-redirect]")].map((v) => [v.videoWidth, v.videoHeight]));
  console.log("CLIENT VIDEO", JSON.stringify(mimes));

  if (MEASURE) {
    const hostPid = Number(execFileSync("pgrep", ["-o", "-f", "--", `--user-data-dir=${profile}`]).toString().trim());
    const measure = async (label, style) => {
      await player.evaluate((css) => {
        const v = [...document.querySelectorAll("video")].find((v) => !v.paused);
        v.style.removeProperty("filter");
        v.style.removeProperty("visibility");
        for (const [name, value] of Object.entries(css)) v.style.setProperty(name, value, "important");
      }, style);
      await shell.waitForTimeout(2000);
      const before = [cpuOfTree(hostPid), cpuOfTree(agent.child.pid)];
      await shell.waitForTimeout(10000);
      const after = [cpuOfTree(hostPid), cpuOfTree(agent.child.pid)];
      const rss = Number(readFileSync(`/proc/${agent.child.pid}/statm`, "utf8").split(" ")[1]) * 4096;
      console.log(`MEASURE: ${label}: host browser ${(((after[0] - before[0]) / 10) * 100).toFixed(0)} % of a core, agent ${(((after[1] - before[1]) / 10) * 100).toFixed(0)} %, agent memory ${Math.round(rss / 1e6)} MB`);
    };
    await measure("video drawn on the host", {});
    await measure("video drawn black (filter)", { filter: "brightness(0)" });
    await measure("video hidden with a black box (the default)", { visibility: "hidden" });
  }
  check(errors.length === 0, `no page error ${errors.join("; ")}`);
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
  if (process.env.FARGLASS_DEBUG) {
    const medias = await browser?.contexts()[0]?.pages()[0]?.evaluate(() =>
      [...document.querySelectorAll('video[data-redirect]')].map((m) => ({ id: m.dataset.redirect, ready: m.readyState, time: m.currentTime, paused: m.paused, error: m.error?.message, host: m.dataset.hostTime, hostPaused: m.dataset.hostPaused, buffered: [...Array(m.buffered.length).keys()].map((i) => [m.buffered.start(i), m.buffered.end(i)]) })),
    ).catch((e) => String(e));
    console.error("MEDIAS", JSON.stringify(medias));
    const hostState = await host?.pages()[0]?.evaluate(() => ({ visibility: document.visibilityState, videos: [...document.querySelectorAll("video")].map((v) => ({ paused: v.paused, time: v.currentTime, ready: v.readyState, muted: v.muted })), ad: !!document.querySelector(".ad-showing"), overlay: document.querySelector(".ytp-error, .ytp-pause-overlay, tp-yt-paper-dialog")?.textContent?.slice(0, 200) })).catch((e) => String(e));
    console.error("HOST STATE", JSON.stringify(hostState));
    if (SHOTS) await host?.pages()[0]?.screenshot({ path: join(SHOTS, "youtube-host.png") }).catch(() => {});
    const titles = await browser?.contexts()[0]?.pages()[0]?.evaluate(() => [...document.querySelectorAll(".wm-window")].map((w) => w.getAttribute("aria-label"))).catch(() => []);
    console.error("WINDOWS", JSON.stringify(titles));
  }
  if (process.env.FARGLASS_DEBUG) console.error(agentLog().split("\n").filter((line) => /redirect|host browser|WARN|ERROR/i.test(line)).join("\n"));
  if (SHOTS) await browser?.contexts()[0]?.pages()[0]?.screenshot({ path: join(SHOTS, "redirect-fail.png") }).catch(() => {});
} finally {
  await host?.close().catch(() => {});
  await browser?.close();
  server?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
