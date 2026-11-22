// border.mjs - end-to-end test of the window border of a host browser with a redirected video
//
// Usage: npm --prefix spikes/e2e run border
//
// The host Chromium plays a video over the full width of its page, as YouTube
// does, in the smart mode. The script moves the pointer on and off the video,
// gives the focus to a second window and back, and moves that window over the
// browser. After each action it samples the left and the right border column
// of the browser window and compares each pixel with the border color.
//
// Exit status: 0 if the border is complete in each sample, 1 if not. The
// script skips the test with no GStreamer VP8 encoder in Python.

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "playwright";
import { buildAll, launch, root, start, TIMEOUT_MS, raise } from "./lib.mjs";

const SHOTS = process.env.FARGLASS_SHOTS;
const EXTENSION = join(root, "extension", "redirect");
const state = mkdtempSync(join(tmpdir(), "farglass-e2e-"));
const children = [];
let host;
let browser;
let server;
let failed = false;

try {
  try {
    execFileSync("python3", ["-c", "import gi; gi.require_version('Gst', '1.0'); from gi.repository import Gst; Gst.init(None); assert Gst.ElementFactory.find('vp8enc')"]);
  } catch {
    console.log("SKIP: no GStreamer VP8 encoder in Python");
    process.exit(0);
  }
  const clip = join(state, "clip.webm");
  execFileSync("python3", [
    "-c",
    `import gi; gi.require_version('Gst', '1.0'); from gi.repository import Gst; Gst.init(None)
p = Gst.parse_launch('videotestsrc num-buffers=900 pattern=smpte horizontal-speed=3 ! video/x-raw,width=640,height=360,framerate=30/1 ! vp8enc deadline=1 target-bitrate=1500000 keyframe-max-dist=30 ! webmmux ! filesink location=${clip}')
p.set_state(Gst.State.PLAYING); p.get_bus().timed_pop_filtered(Gst.CLOCK_TIME_NONE, Gst.MessageType.EOS | Gst.MessageType.ERROR); p.set_state(Gst.State.NULL)`,
  ]);
  // The video takes the full width of the page, and the page scrolls, as on YouTube.
  const page = `<!doctype html><title>Border test</title>
<style>body { margin: 0; background: #0f0f0f; color: #fff; font: 16px sans-serif; } video { display: block; width: 100%; aspect-ratio: 16 / 9; background: #000; } p { height: 1500px; }</style>
<video id="v" muted></video><p>Below the video.</p>
<script>
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
  await agent.match;

  browser = await chromium.launch();
  const shell = await (await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1400, height: 900 } })).newPage();
  await shell.goto(url);
  await shell.locator(".wm").waitFor({ timeout: TIMEOUT_MS });

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

  const window = shell.locator('.wm-window[aria-label^="Border test"]');
  await window.waitFor({ timeout: 60_000 });
  await raise(shell, window);
  await shell.waitForFunction(() => {
    const v = document.querySelector(".wm-window video[data-redirect]");
    return v && v.readyState >= 2 && !v.paused && v.currentTime > 0.5;
  }, null, { timeout: 30_000 });
  await shell.evaluate(() => globalThis.farglassVideoMode("smart"));
  console.log("PASS: the client plays the video of the host in the smart mode");

  await launch(shell, "Settings");
  const other = shell.locator('.wm-window[aria-label^="Settings"]').first();
  await other.waitFor({ timeout: TIMEOUT_MS });
  await raise(shell, window);

  /** Returns the defects of the left and the right border column of the browser window. */
  const faults = [];
  let samples = 0;
  let focusedSamples = 0;
  const overflow = new Set();
  const sample = async (label) => {
    const result = await window.evaluate((element) => {
      const box = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return { x: box.x, y: box.y, width: box.width, height: box.height, color: style.borderLeftColor, focused: element.classList.contains("wm-focused") };
    });
    const video = await window.locator("video[data-redirect]").boundingBox();
    const spill = await window.evaluate((element) => {
      const main = element.querySelector(".linux-main").getBoundingClientRect();
      return [...element.querySelectorAll("video[data-redirect]")].map((v) => {
        const r = v.getBoundingClientRect();
        return [main.left - r.left, r.right - main.right, main.top - r.top, r.bottom - main.bottom].map((n) => Math.round(n * 10) / 10).join(",");
      });
    });
    for (const s of spill) overflow.add(s);
    const top = Math.ceil(result.y + 40);
    const bottom = Math.floor(result.y + result.height - 8);
    const columns = [
      ["left", Math.floor(result.x)],
      ["right", Math.ceil(result.x + result.width) - 1],
    ];
    for (const [side, x] of columns) {
      const shot = await shell.screenshot({ clip: { x, y: top, width: 1, height: bottom - top } });
      const bad = await shell.evaluate(
        async ({ b64, color }) => {
          const [r, g, b] = color.match(/\d+/g).map(Number);
          const image = new Image();
          image.src = `data:image/png;base64,${b64}`;
          await image.decode();
          const canvas = new OffscreenCanvas(image.width, image.height);
          const context = canvas.getContext("2d");
          context.drawImage(image, 0, 0);
          const data = context.getImageData(0, 0, image.width, image.height).data;
          const rows = [];
          for (let i = 0; i < image.height; i += 1) {
            const d = Math.abs(data[i * 4] - r) + Math.abs(data[i * 4 + 1] - g) + Math.abs(data[i * 4 + 2] - b);
            if (d > 60) rows.push(i);
          }
          return rows;
        },
        { b64: shot.toString("base64"), color: result.color },
      );
      // Another window or its shadow over the column covers the border, as it must: only rows far from other windows count.
      const covered = await shell.evaluate(
        ({ x, ys, own }) => {
          const others = [...document.querySelectorAll(".wm-window:not([hidden])")]
            .filter((w) => w.getAttribute("aria-label") !== own)
            .map((w) => w.getBoundingClientRect());
          const shadow = 40;
          return ys.filter((y) => others.some((r) => x >= r.left - shadow && x <= r.right + shadow && y >= r.top - shadow && y <= r.bottom + shadow));
        },
        { x, ys: bad.map((row) => row + top), own: await window.getAttribute("aria-label") },
      );
      const real = bad.map((row) => row + top).filter((y) => !covered.includes(y));
      samples += 1;
      if (result.focused) focusedSamples += 1;
      if (real.length > 0) {
        const inVideo = video ? real.filter((y) => y >= video.y && y <= video.y + video.height).length : 0;
        faults.push(`${label}: ${side} border (x ${x}, focused ${result.focused}) misses ${real.length} rows from y ${real[0]} to ${real.at(-1)}, ${inVideo} at the height of the video`);
        if (SHOTS) await shell.screenshot({ path: join(SHOTS, `border-${faults.length}.png`) });
      }
    }
  };
  const burst = async (label, count = 8) => {
    for (let i = 0; i < count; i += 1) {
      await sample(label);
      await shell.waitForTimeout(60);
    }
  };

  const videoBox = await window.locator("video[data-redirect]").boundingBox();
  const centre = { x: videoBox.x + videoBox.width / 2, y: videoBox.y + videoBox.height / 2 };
  const titleOf = async (w) => (await w.boundingBox());
  for (let round = 0; round < 3; round += 1) {
    await shell.mouse.move(5, 880);
    await burst(`round ${round}: pointer off, client draws`);
    await shell.mouse.move(centre.x, centre.y, { steps: 3 });
    await burst(`round ${round}: pointer on the video, host draws`);
    for (let i = 0; i < 6; i += 1) {
      await shell.mouse.move(centre.x + (i % 2) * 10, centre.y + 5, { steps: 2 });
      await sample(`round ${round}: pointer moves on the video`);
    }
    // Focus to the other window, and back with a click on the page of the browser.
    await raise(shell, other);
    await burst(`round ${round}: browser not focused`);
    // The other window over the right half of the browser.
    const bar = await titleOf(other.locator(".wm-titlebar"));
    const target = await window.boundingBox();
    await shell.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 2);
    await shell.mouse.down();
    // The middle of the title bar to the right part of the browser: the left half of the video stays free for the click.
    await shell.mouse.move(target.x + target.width * 0.7 + bar.width / 2, target.y + target.height * 0.3, { steps: 8 });
    await shell.mouse.up();
    await burst(`round ${round}: other window over the browser`);
    await shell.mouse.click(centre.x - videoBox.width / 4, centre.y);
    await burst(`round ${round}: browser focused again by a click on the video`);
    await shell.mouse.move(centre.x - videoBox.width / 4 + 8, centre.y, { steps: 2 });
    await shell.waitForTimeout(3500);
    await burst(`round ${round}: 3 s with no movement, client draws again`);
    // The other window away again.
    await raise(shell, other);
    const bar2 = await titleOf(other.locator(".wm-titlebar"));
    await shell.mouse.move(bar2.x + bar2.width / 2, bar2.y + bar2.height / 2);
    await shell.mouse.down();
    await shell.mouse.move(bar2.width / 2 + 1000, 80, { steps: 8 });
    await shell.mouse.up();
    await raise(shell, window);
    await burst(`round ${round}: browser focused through the panel`);
  }
  console.log(`${samples} border samples, ${focusedSamples} with the browser focused`);
  console.log(`the client video past the app area (left, right, top, bottom px): ${[...overflow].join(" | ")}`);
  if (faults.length > 0) {
    failed = true;
    for (const fault of faults) console.log(`FAIL: ${fault}`);
  } else {
    console.log("PASS: the border of the browser window is complete in each sample");
  }
} catch (error) {
  failed = true;
  console.error(`FAIL: ${error.message}`);
  if (SHOTS) await browser?.contexts()[0]?.pages()[0]?.screenshot({ path: join(SHOTS, "border-fail.png") }).catch(() => {});
} finally {
  await host?.close().catch(() => {});
  await browser?.close();
  server?.close();
  for (const child of children) child.kill("SIGKILL");
  rmSync(state, { recursive: true, force: true });
}

process.exit(failed ? 1 : 0);
