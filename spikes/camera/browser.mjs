// browser.mjs - test if a browser on the host shows a PipeWire node as a camera (F115)
//
// Usage: node browser.mjs chromium|firefox LABEL [ARGS...]
//
// The script opens a page on http://localhost, lists the cameras, and opens
// the camera with the label LABEL. It prints the labels, the size of the
// track, the frame rate that the page counts, and the mean of the pixels.
// The browser gets the extra launch arguments ARGS. With KEEP_MS, the page
// keeps the camera open for that time after the measurement.
//
// Exit status: 0 if the page gets frames from LABEL, 1 if not.

import { createServer } from "node:http";
import { chromium, firefox } from "../e2e/node_modules/playwright/index.mjs";

const [engine, label, ...args] = process.argv.slice(2);
const server = createServer((_req, res) => {
  res.setHeader("content-type", "text/html");
  res.end("<!doctype html><video autoplay muted playsinline></video>");
}).listen(0, "127.0.0.1");
await new Promise((resolve) => server.once("listening", resolve));
const url = `http://localhost:${server.address().port}/`;

const browser =
  engine === "firefox"
    ? await firefox.launch({
        args,
        firefoxUserPrefs: {
          "media.webrtc.camera.allow-pipewire": true,
          "media.navigator.permission.disabled": true,
        },
      })
    : // The full Chromium, not the headless shell of Playwright.
      await chromium.launch({ channel: "chromium", args: ["--use-fake-ui-for-media-stream", ...args] });

let status = 1;
try {
  const context = await browser.newContext();
  if (engine !== "firefox") await context.grantPermissions(["camera"], { origin: url });
  const page = await context.newPage();
  page.on("console", (msg) => console.log(`page: ${msg.text()}`));
  console.log(`browser: started at ${new Date().toISOString()}`);
  await page.goto(url);
  const result = await page.evaluate(async ([label, keep]) => {
    // A first getUserMedia gives the labels to enumerateDevices.
    const probe = await navigator.mediaDevices.getUserMedia({ video: true });
    probe.getTracks().forEach((t) => t.stop());
    const devices = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
    const labels = devices.map((d) => d.label);
    const device = devices.find((d) => d.label.includes(label));
    if (!device) return { labels };
    const stream = await navigator.mediaDevices.getUserMedia({
      video: { deviceId: { exact: device.deviceId }, width: 1280, height: 720, frameRate: 30 },
    });
    const video = document.querySelector("video");
    video.srcObject = stream;
    await new Promise((resolve) => (video.onplaying = resolve));
    let frames = 0;
    const start = performance.now();
    await new Promise((resolve) => {
      const step = () => {
        frames += 1;
        if (performance.now() - start > 3000) resolve();
        else video.requestVideoFrameCallback(step);
      };
      video.requestVideoFrameCallback(step);
    });
    const fps = frames / ((performance.now() - start) / 1000);
    const canvas = Object.assign(document.createElement("canvas"), { width: video.videoWidth, height: video.videoHeight });
    const ctx = canvas.getContext("2d");
    ctx.drawImage(video, 0, 0);
    const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let sum = 0;
    for (let i = 0; i < pixels.length; i += 4) sum += pixels[i] + pixels[i + 1] + pixels[i + 2];
    const settings = stream.getVideoTracks()[0].getSettings();
    console.log(`measured fps ${fps.toFixed(1)}`);
    await new Promise((resolve) => setTimeout(resolve, keep));
    stream.getTracks().forEach((t) => t.stop());
    return { labels, width: settings.width, height: settings.height, fps, mean: sum / (pixels.length / 4) / 3 };
  }, [label, Number(process.env.KEEP_MS ?? 0)]);
  console.log(JSON.stringify(result));
  // HOLD_MS keeps the browser open, so pw-dump can show its PipeWire client.
  await new Promise((resolve) => setTimeout(resolve, Number(process.env.HOLD_MS ?? 0)));
  if (result.width && result.fps > 20 && result.mean > 1) status = 0;
} catch (err) {
  console.log(`error: ${err.message.split("\n")[0]}`);
} finally {
  await browser.close();
  server.close();
}
process.exit(status);
