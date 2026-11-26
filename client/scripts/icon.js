// icon.js - render web/src/brand/farglass.svg to the PNG icons of the desktop client (F121)
//
// Usage: npx electron scripts/icon.js
//
// Writes build/icons/SIZExSIZE.png for the packages and static/tray.png for the tray.
// The PNG files are in git. Run the script again after a change of the SVG.
const { app, BrowserWindow, nativeImage } = require("electron");
const { mkdirSync, readFileSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");

const root = join(__dirname, "..");
const svg = readFileSync(join(root, "..", "web", "src", "brand", "farglass.svg"), "utf8");
const SIZE = 512;

app.disableHardwareAcceleration();
app.whenReady().then(async () => {
  const win = new BrowserWindow({ width: SIZE, height: SIZE, show: false, frame: false, transparent: true, webPreferences: { offscreen: true } });
  const html = `<html><body style="margin:0;background:transparent"><img style="width:${SIZE}px;height:${SIZE}px;display:block" src="data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}"></body></html>`;
  await win.loadURL(`data:text/html;base64,${Buffer.from(html).toString("base64")}`);
  await new Promise((resolve) => setTimeout(resolve, 500));
  const image = nativeImage.createFromBuffer((await win.webContents.capturePage()).toPNG());
  mkdirSync(join(root, "build", "icons"), { recursive: true });
  for (const size of [16, 32, 48, 64, 128, 256, 512]) {
    writeFileSync(join(root, "build", "icons", `${size}x${size}.png`), image.resize({ width: size, height: size, quality: "best" }).toPNG());
  }
  writeFileSync(join(root, "static", "tray.png"), image.resize({ width: 32, height: 32, quality: "best" }).toPNG());
  app.quit();
});
