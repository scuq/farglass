// redirect.mjs - end-to-end test of video redirection (F039, F040)
//
// Usage: npm --prefix spikes/e2e run redirect
//        FARGLASS_MEASURE=1 npm --prefix spikes/e2e run redirect
//
// The script makes a VP8 WebM clip with GStreamer and serves a page that
// plays it through Media Source Extensions. A Chromium on the Wayland
// compositor of the agent is the host browser, with the farglass extension
// and its native messaging host. The farglass shell in a second Chromium is
// the client. The client must play the same stream in a video element under
// the window, the host must draw its video black, and the pause, the seek,
// and the play of the host must reach the client.
//
// With FARGLASS_MEASURE=1, the script also measures the CPU of the host
// browser and of the agent with the video drawn, drawn black, and hidden.
//
// Exit status: 0 if all steps pass, 1 if a step fails. The script skips the
// test with no GStreamer VP8 encoder in Python.

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, root, start, TIMEOUT_MS, raise } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const MEASURE = process.env.FARGLASS_MEASURE === "1";
const EXTENSION = join(root, "extension", "redirect");
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
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
  try {
    execFileSync("python3", ["-c", "import gi; gi.require_version('Gst', '1.0'); from gi.repository import Gst; Gst.init(None); assert Gst.ElementFactory.find('vp8enc')"]);
  } catch {
    console.log("SKIP: no GStreamer VP8 encoder in Python");
    process.exit(0);
  }
  // A clip of 20 s: colored bars that move, 640 x 360, VP8 in WebM.
  const clip = join(state, "clip.webm");
  execFileSync("python3", [
    "-c",
    `import gi; gi.require_version('Gst', '1.0'); from gi.repository import Gst; Gst.init(None)
p = Gst.parse_launch('videotestsrc num-buffers=600 pattern=smpte horizontal-speed=3 ! video/x-raw,width=640,height=360,framerate=30/1 ! vp8enc deadline=1 target-bitrate=1500000 keyframe-max-dist=30 ! webmmux ! filesink location=${clip}')
p.set_state(Gst.State.PLAYING); p.get_bus().timed_pop_filtered(Gst.CLOCK_TIME_NONE, Gst.MessageType.EOS | Gst.MessageType.ERROR); p.set_state(Gst.State.NULL)`,
  ]);
  const page = `<!doctype html><title>Redirect test</title>
<style>body { margin: 0; font: 16px sans-serif; } body.black { background: #000; color: #fff; } video { display: block; width: 640px; height: 360px; margin: 20px; background: #000; }</style>
<video id="v" muted></video><p>A page with a video of Media Source Extensions.</p>
<script>
if (location.search === "?black") document.body.classList.add("black");
const video = document.getElementById("v");
const source = new MediaSource();
video.src = URL.createObjectURL(source);
source.addEventListener("sourceopen", async () => {
  const buffer = source.addSourceBuffer('video/webm; codecs="vp8"');
  const bytes = new Uint8Array(await (await fetch("/clip.webm")).arrayBuffer());
  for (let i = 0; i < bytes.length; i += 262144) {
    buffer.appendBuffer(bytes.subarray(i, i + 262144));
    await new Promise((resolve) => buffer.addEventListener("updateend", resolve, { once: true }));
  }
  source.endOfStream();
  video.loop = true;
  video.play();
});
</script>`;
  server = http.createServer((request, response) => {
    if (request.url === "/clip.webm") {
      response.writeHead(200, { "content-type": "video/webm" });
      response.end(readFileSync(clip));
    } else {
      response.writeHead(200, { "content-type": "text/html" });
      response.end(page);
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const pageUrl = `http://127.0.0.1:${server.address().port}/`;

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
  const shell = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1400, height: 900 } })).newPage();
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
    args: ["--ozone-platform=wayland", `--disable-extensions-except=${EXTENSION}`, `--load-extension=${EXTENSION}`, "--window-size=900,700", "--autoplay-policy=no-user-gesture-required"],
    env: { ...process.env, WAYLAND_DISPLAY: wayland, DISPLAY: "", FARGLASS_REDIRECT_SOCKET: redirectSocket },
  });
  const player = host.pages()[0] ?? (await host.newPage());
  await player.goto(pageUrl);
  await player.waitForFunction(() => document.getElementById("v").currentTime > 0.5, null, { timeout: TIMEOUT_MS });
  check(true, "the host page plays its MSE video");

  // 1. The client plays the same stream in a video element in the window of the host browser.
  const window = shell.locator('.wm-window[aria-label^="Redirect test"]');
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

  // 2. The host draws its video black, and the client shows its own video through the canvas.
  await player.waitForFunction(() => getComputedStyle(document.getElementById("v")).filter.includes("brightness(0)"), null, { timeout: TIMEOUT_MS });
  check(true, "the host draws the video black while the client shows it: the page under it is white");
  const box = await video.boundingBox();
  check(box && box.width > 600 && box.width < 680, `the client video has the size of the player (${box?.width}x${box?.height})`);
  await shell.waitForTimeout(1500);
  const shot = await shell.screenshot({ clip: { x: box.x + box.width / 4, y: box.y + box.height / 4, width: 8, height: 8 } });
  if (SHOTS) {
    writeFileSync(join(SHOTS, "redirect-sample.png"), shot);
    await shell.screenshot({ path: join(SHOTS, "redirect.png") });
  }
  const colored = await shell.evaluate(async (b64) => {
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
  check(colored > 60, `the bars of the video show through the black of the host (brightness ${colored.toFixed(0)})`);

  // 3. Pause, seek, and play of the host reach the client.
  await player.evaluate(() => document.getElementById("v").pause());
  await shell.waitForFunction(() => document.querySelector(".wm-window video[data-redirect]").paused, null, { timeout: 3000 });
  check(true, "a pause on the host pauses the client");
  await player.evaluate(() => {
    document.getElementById("v").currentTime = 12;
  });
  await shell.waitForFunction(() => Math.abs(document.querySelector(".wm-window video[data-redirect]").currentTime - 12) < 0.5, null, { timeout: 3000 });
  check(true, "a seek on the host seeks the client to 12 s");
  await player.evaluate(() => document.getElementById("v").play());
  await shell.waitForFunction(() => {
    const v = document.querySelector(".wm-window video[data-redirect]");
    return !v.paused && v.currentTime > 12.3;
  }, null, { timeout: 3000 });
  const hostNow = await player.evaluate(() => document.getElementById("v").currentTime);
  const clientNow = await video.evaluate((v) => v.currentTime);
  check(Math.abs(hostNow - clientNow) < 0.6, `a play on the host plays the client in step (host ${hostNow.toFixed(2)} s, client ${clientNow.toFixed(2)} s)`);

  // 4. On a page that is black under the video, the host hides the video: nothing to draw, nothing to encode.
  await player.goto(`${pageUrl}?black`);
  await player.waitForFunction(() => getComputedStyle(document.getElementById("v")).visibility === "hidden", null, { timeout: 30_000 });
  await shell.waitForFunction(() => {
    const v = document.querySelector(".wm-window video[data-redirect]");
    return v && !v.paused && v.currentTime > 0.5;
  }, null, { timeout: 30_000 });
  check(true, "on a black page the host hides its video, and the client plays the new stream");
  await shell.waitForTimeout(1000);
  const blackBox = await window.locator("video[data-redirect]").boundingBox();
  const sample = await shell.screenshot({ clip: { x: blackBox.x + blackBox.width / 4, y: blackBox.y + blackBox.height / 4, width: 4, height: 4 } });
  if (SHOTS) await shell.screenshot({ path: join(SHOTS, "redirect-black.png") });
  check(!sample.equals(await shell.screenshot({ clip: { x: 5, y: 880, width: 4, height: 4 } })), "the client video shows through on the black page");

  if (MEASURE) {
    const hostPid = Number(execFileSync("pgrep", ["-o", "-f", "--", `--user-data-dir=${profile}`]).toString().trim());
    const measure = async (label, style) => {
      await player.evaluate((css) => {
        const v = document.getElementById("v");
        v.style.removeProperty("filter");
        v.style.removeProperty("visibility");
        for (const [name, value] of Object.entries(css)) v.style.setProperty(name, value, "important");
      }, style);
      await shell.waitForTimeout(2000);
      const before = [cpuOfTree(hostPid), cpuOfTree(agent.child.pid)];
      await shell.waitForTimeout(10000);
      const after = [cpuOfTree(hostPid), cpuOfTree(agent.child.pid)];
      console.log(`MEASURE: ${label}: host browser ${(((after[0] - before[0]) / 10) * 100).toFixed(0)} % of a core, agent ${(((after[1] - before[1]) / 10) * 100).toFixed(0)} %`);
    };
    await measure("video drawn on the host", {});
    await measure("video drawn black (filter)", { filter: "brightness(0)" });
    await measure("video hidden (visibility)", { visibility: "hidden" });
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
