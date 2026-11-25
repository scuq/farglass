// firefox.mjs - test if the Firefox of the host shows a PipeWire node as a camera (F115)
//
// Usage: node firefox.mjs LABEL [FIREFOX]
//
// Playwright's Firefox lists V4L2 cameras only, so this script starts the
// Firefox of the host (default firefox-esr) in headless mode with a new
// profile. The profile turns on media.webrtc.camera.allow-pipewire (not with
// PIPEWIRE=default) and turns
// off the permission prompt. The page opens the camera with the label LABEL
// and posts its result to the script, which prints it.
//
// Exit status: 0 if the page gets frames from LABEL, 1 if not.

import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [label, binary = "firefox-esr"] = process.argv.slice(2);

const page = `<!doctype html><video autoplay muted></video><script>
(async () => {
  const report = (r) => fetch("/result", { method: "POST", body: JSON.stringify(r) });
  try {
    const probe = await navigator.mediaDevices.getUserMedia({ video: true });
    probe.getTracks().forEach((t) => t.stop());
    const devices = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
    const labels = devices.map((d) => d.label);
    const device = devices.find((d) => d.label.includes(${JSON.stringify(label)}));
    if (!device) return report({ labels });
    const stream = await navigator.mediaDevices.getUserMedia({
      video: { deviceId: { exact: device.deviceId }, width: 1280, height: 720 },
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
    report({ labels, width: settings.width, height: settings.height, fps, mean: sum / (pixels.length / 4) / 3 });
  } catch (err) {
    report({ error: String(err) });
  }
})();
</script>`;

let done;
const result = new Promise((resolve) => (done = resolve));
const server = createServer((req, res) => {
  if (req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      res.end();
      done(JSON.parse(body));
    });
    return;
  }
  res.setHeader("content-type", "text/html");
  res.end(page);
}).listen(0, "127.0.0.1");
await new Promise((resolve) => server.once("listening", resolve));

const profile = mkdtempSync(join(tmpdir(), "farglass-firefox-"));
writeFileSync(
  join(profile, "user.js"),
  [
    // PIPEWIRE=default keeps the default value of the pref, to test it.
    process.env.PIPEWIRE === "default" ? "" : 'user_pref("media.webrtc.camera.allow-pipewire", true);',
    'user_pref("media.navigator.permission.disabled", true);',
    'user_pref("media.navigator.streams.fake", false);',
    'user_pref("browser.shell.checkDefaultBrowser", false);',
    'user_pref("datareporting.policy.dataSubmissionEnabled", false);',
  ].join("\n"),
);
const firefox = spawn(binary, ["--headless", "--no-remote", "--profile", profile, `http://localhost:${server.address().port}/`], {
  stdio: "ignore",
});
const timeout = new Promise((resolve) => setTimeout(() => resolve({ error: "no result after 40 s" }), 40_000));
const outcome = await Promise.race([result, timeout]);
console.log(JSON.stringify(outcome));
firefox.kill();
server.close();
process.exit(outcome.width && outcome.fps > 20 && outcome.mean > 1 ? 0 : 1);
