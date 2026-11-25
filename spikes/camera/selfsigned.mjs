// selfsigned.mjs - test getUserMedia on a page with a self-signed certificate (F115)
//
// Usage: node selfsigned.mjs
//
// The script makes a self-signed certificate with openssl, as farglassd does
// on its first start (ADR-016), and serves a page over HTTPS at
// https://farglass.cyberdyne.example. The client Chromium shows the warning of the certificate.
// The script clicks Advanced and then Proceed, as a user does, and then
// opens the fake camera and the fake microphone of Chromium.
//
// Exit status: 0 if the page is a secure context and gets both tracks, 1 if not.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { createServer } from "node:https";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium } from "../e2e/node_modules/playwright/index.mjs";

const host = "farglass.cyberdyne.example";
const dir = mkdtempSync(join(tmpdir(), "farglass-cert-"));
execFileSync("openssl", [
  "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256", "-nodes", "-days", "1",
  "-subj", `/CN=${host}`, "-addext", `subjectAltName=DNS:${host}`,
  "-keyout", join(dir, "key.pem"), "-out", join(dir, "cert.pem"),
], { stdio: "ignore" });

const server = createServer(
  { key: readFileSync(join(dir, "key.pem")), cert: readFileSync(join(dir, "cert.pem")) },
  (_req, res) => {
    res.setHeader("content-type", "text/html");
    res.end("<!doctype html><title>farglass</title><p>page");
  },
).listen(0, "127.0.0.1");
await new Promise((resolve) => server.once("listening", resolve));
const url = `https://${host}:${server.address().port}/`;

const browser = await chromium.launch({
  channel: "chromium",
  args: [
    `--host-resolver-rules=MAP ${host} 127.0.0.1`,
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
  ],
});
let status = 1;
try {
  const page = await browser.newPage();
  await page.goto(url).catch(() => {});
  const warning = await page.locator("#details-button").waitFor({ timeout: 10_000 }).then(() => true, () => false);
  console.log(`warning shows: ${warning}`);
  await page.click("#details-button");
  await page.click("#proceed-link");
  await page.waitForSelector("text=page");
  const result = await page.evaluate(async () => {
    const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
    const tracks = stream.getTracks().map((t) => `${t.kind}:${t.readyState}`);
    stream.getTracks().forEach((t) => t.stop());
    return { secure: window.isSecureContext, tracks, encoder: typeof VideoEncoder };
  });
  console.log(JSON.stringify(result));
  if (warning && result.secure && result.tracks.length === 2) status = 0;
} catch (err) {
  console.log(`error: ${err.message.split("\n")[0]}`);
} finally {
  await browser.close();
  server.close();
}
process.exit(status);
