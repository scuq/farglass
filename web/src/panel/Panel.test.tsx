import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { closeFrame, dataFrame, encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { Desktop } from "../wm/Desktop";
import { emptyDesktop, minimizeWindow, openWindow } from "../wm/model";
import { Clock } from "./Clock";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function mountDesktop(onlogout = () => {}) {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <Desktop connection={connection} user="alice" onlogout={onlogout} />, host);
  const opened = server.take().filter((f) => f.type === FrameType.Open);
  const channel = opened.find((f) => parseJson<{ kind: string }>(f).kind === "state")!.channel;
  server.push(readyFrame(channel));
  // An empty desktop that is not new: the desktop opens no window.
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "desktop", value: emptyDesktop() }));
  await settle();
  const channelOf = (kind: string) => opened.find((f) => parseJson<{ kind: string }>(f).kind === kind)!.channel;
  return { host, server, channelOf };
}

const byText = (host: HTMLElement, selector: string, text: string) =>
  [...host.querySelectorAll<HTMLElement>(selector)].find((element) => element.textContent?.includes(text))!;

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function key(type: "keydown" | "keyup", init: KeyboardEventInit, target: EventTarget = window) {
  target.dispatchEvent(new KeyboardEvent(type, { bubbles: true, cancelable: true, ...init }));
}

describe("activity of minimized windows", () => {
  test("a minimized window shows if its app is busy or idle, with the numbers in the tooltip", async () => {
    const { server, connection } = await fakeConnection("alice");
    const host = document.createElement("div");
    document.body.append(host);
    dispose = render(() => <Desktop connection={connection} user="alice" onlogout={() => {}} />, host);
    const opened = server.take().filter((f) => f.type === FrameType.Open);
    const channelOf = (kind: string) => opened.find((f) => parseJson<{ kind: string }>(f).kind === kind)!.channel;
    const state = channelOf("state");
    server.push(readyFrame(state));
    const area = { x: 0, y: 0, w: 1024, h: 768 };
    let desktop = emptyDesktop();
    for (const id of ["busy", "idle", "shown"]) desktop = openWindow(desktop, { id, app: "terminal", title: `T ${id}`, w: 300, h: 200 }, area);
    desktop = minimizeWindow(minimizeWindow(desktop, "busy"), "idle");
    server.push(encodeJsonFrame(state, FrameType.Control, { type: "value", key: "desktop", value: desktop }));
    const stats = channelOf("stats");
    server.push(readyFrame(stats));
    const usage = (cpu: number) => ({ cpu, memory: 340_000_000, net: 1_200_000 * (cpu > 10 ? 1 : 0) });
    server.push(
      encodeJsonFrame(stats, FrameType.Control, {
        type: "sample",
        windows: [
          { window: "busy", usage: usage(42) },
          { window: "idle", usage: usage(0.2) },
          { window: "shown", usage: usage(90) },
        ],
        agent: usage(1),
      }),
    );
    await settle();
    const task = (title: string) => byText(host, ".wm-task", title);
    expect(task("T busy").dataset.activity).toBe("busy");
    expect(task("T busy").title).toBe("T busy: busy, CPU 42 %, memory 340 MB, network 1.2 MB/s");
    expect(task("T idle").dataset.activity).toBe("idle");
    expect(task("T idle").title).toContain("idle, CPU 0 %");
    // A window that shows needs no mark: its content shows what the app does.
    expect(task("T shown").dataset.activity).toBeUndefined();
    expect(task("T shown").title).toBe("T shown: busy, CPU 90 %, memory 340 MB, network 1.2 MB/s");
  });
});

describe("sound of the apps", () => {
  test("the sound button shows with a sound sink, counts the packets, and mutes", async () => {
    const decoded: unknown[] = [];
    const gains: { value: number }[] = [];
    class FakeContext {
      audioWorklet = { addModule: async () => {} };
      destination = {};
      createGain() {
        const gain = { gain: { value: 1 }, connect: (to: unknown) => to };
        gains.push(gain.gain);
        return gain;
      }
      resume = async () => {};
      close = async () => {};
    }
    class FakeNode {
      port = { onmessage: null, postMessage: () => {} };
      connect = (to: unknown) => to;
    }
    class FakeDecoder {
      state = "unconfigured";
      configure() {
        this.state = "configured";
      }
      decode(chunk: unknown) {
        decoded.push(chunk);
      }
      close() {}
    }
    class FakeChunk {
      constructor(readonly init: { timestamp: number }) {}
    }
    vi.stubGlobal("AudioContext", FakeContext);
    vi.stubGlobal("AudioWorkletNode", FakeNode);
    vi.stubGlobal("AudioDecoder", FakeDecoder);
    vi.stubGlobal("EncodedAudioChunk", FakeChunk);
    try {
      const { host, server, channelOf } = await mountDesktop();
      const audio = channelOf("audio");
      server.push(readyFrame(audio));
      expect(host.querySelector(".panel-sound")).toBeNull();
      server.push(encodeJsonFrame(audio, FrameType.Control, { type: "format", codec: "opus", sample_rate: 48000, channels: 2, frame_samples: 960 }));
      await settle();
      await settle();
      const button = host.querySelector<HTMLButtonElement>(".panel-sound")!;
      expect(button.textContent).toBe("Sound");
      server.push(dataFrame(audio, new Uint8Array([7, 0, 0, 0, 0xfc, 0xff, 0xfe])));
      await settle();
      expect(decoded).toHaveLength(1);
      expect((decoded[0] as FakeChunk).init.timestamp).toBe(7 * 20_000);
      expect(button.dataset.packets).toBe("1");
      button.click();
      await settle();
      expect(button.getAttribute("aria-pressed")).toBe("true");
      expect(button.textContent).toBe("Sound off");
      expect(gains[0]!.value).toBe(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("microphone for the apps", () => {
  function stubMicrophone(getUserMedia: () => Promise<unknown>) {
    const encoded: { timestamp: number; numberOfFrames: number }[] = [];
    const nodes: { port: { onmessage: ((message: { data: Float32Array }) => void) | null } }[] = [];
    const encoders: { output: (chunk: unknown) => void; config?: unknown; state: string }[] = [];
    class FakeContext {
      audioWorklet = { addModule: async () => {} };
      createMediaStreamSource() {
        return { connect: (to: unknown) => to };
      }
      close = async () => {};
    }
    class FakeNode {
      port = { onmessage: null };
      constructor() {
        nodes.push(this);
      }
    }
    class FakeEncoder {
      state = "unconfigured";
      constructor(readonly init: { output: (chunk: unknown) => void }) {
        encoders.push(this as never);
      }
      get output() {
        return this.init.output;
      }
      configure(config: unknown) {
        (this as { config?: unknown }).config = config;
        this.state = "configured";
      }
      encode(data: { timestamp: number; numberOfFrames: number }) {
        encoded.push(data);
      }
      close() {
        this.state = "closed";
      }
    }
    class FakeData {
      constructor(readonly init: { timestamp: number; numberOfFrames: number }) {}
      get timestamp() {
        return this.init.timestamp;
      }
      get numberOfFrames() {
        return this.init.numberOfFrames;
      }
      close() {}
    }
    vi.stubGlobal("AudioContext", FakeContext);
    vi.stubGlobal("AudioWorkletNode", FakeNode);
    vi.stubGlobal("AudioEncoder", FakeEncoder);
    vi.stubGlobal("AudioData", FakeData);
    const media = vi.fn(getUserMedia);
    vi.stubGlobal("navigator", { ...navigator, mediaDevices: { getUserMedia: media } });
    return { encoded, nodes, encoders, media };
  }

  const ready = { type: "ready", codec: "opus", sample_rate: 48000, channels: 1, frame_samples: 960 };

  test("the button turns the microphone on and off, and sends numbered Opus packets", async () => {
    const stopped: string[] = [];
    const track = { stop: () => stopped.push("track"), addEventListener: () => {} };
    const { encoded, nodes, encoders, media } = stubMicrophone(async () => ({ getTracks: () => [track] }));
    try {
      const { host, server, channelOf } = await mountDesktop();
      const mic = channelOf("mic");
      server.push(readyFrame(mic));
      expect(host.querySelector(".panel-mic")).toBeNull();
      server.push(encodeJsonFrame(mic, FrameType.Control, ready));
      await settle();
      const button = host.querySelector<HTMLButtonElement>(".panel-mic")!;
      expect(button.textContent).toBe("Mic off");
      expect(button.getAttribute("aria-pressed")).toBe("false");
      server.take();
      button.click();
      await settle();
      await settle();
      expect(media).toHaveBeenCalledWith({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 } });
      expect(button.textContent).toBe("Mic on");
      expect(button.dataset.state).toBe("on");
      expect(encoders[0]!.config).toEqual({ codec: "opus", sampleRate: 48000, numberOfChannels: 1, bitrate: 32000, opus: { frameDuration: 20000, usedtx: true } });
      const sent = () => server.take().filter((f) => f.channel === mic);
      expect(sent().map((f) => parseJson(f))).toEqual([{ type: "start" }]);
      // Two blocks of 20 ms from the worklet, and two packets from the encoder.
      nodes[0]!.port.onmessage!({ data: new Float32Array(960) });
      nodes[0]!.port.onmessage!({ data: new Float32Array(960) });
      expect(encoded.map((data) => data.timestamp)).toEqual([0, 20_000]);
      const chunk = { byteLength: 2, copyTo: (to: Uint8Array) => to.set([0xfc, 0xff]) };
      encoders[0]!.output(chunk);
      encoders[0]!.output(chunk);
      const packets = sent();
      expect(packets.map((f) => f.type)).toEqual([FrameType.Data, FrameType.Data]);
      expect([...packets[1]!.payload]).toEqual([1, 0, 0, 0, 0xfc, 0xff]);
      button.click();
      await settle();
      expect(sent().map((f) => parseJson(f))).toEqual([{ type: "stop" }]);
      expect(stopped).toEqual(["track"]);
      expect(button.textContent).toBe("Mic off");
      expect(encoders[0]!.state).toBe("closed");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  test("a refusal of the browser shows, a different connection takes the microphone, and a close releases it", async () => {
    let refuse = true;
    const stopped: string[] = [];
    const { media } = stubMicrophone(async () => {
      if (refuse) throw new DOMException("Permission denied", "NotAllowedError");
      return { getTracks: () => [{ stop: () => stopped.push("track"), addEventListener: () => {} }] };
    });
    try {
      const { host, server, channelOf } = await mountDesktop();
      const mic = channelOf("mic");
      server.push(readyFrame(mic));
      server.push(encodeJsonFrame(mic, FrameType.Control, ready));
      await settle();
      const button = host.querySelector<HTMLButtonElement>(".panel-mic")!;
      button.click();
      await settle();
      expect(button.textContent).toBe("Mic blocked");
      expect(button.title).toContain("site settings");
      refuse = false;
      button.click();
      await settle();
      await settle();
      expect(media).toHaveBeenCalledTimes(2);
      expect(button.textContent).toBe("Mic on");
      server.push(encodeJsonFrame(mic, FrameType.Control, { type: "taken" }));
      await settle();
      expect(button.textContent).toBe("Mic off");
      expect(stopped).toEqual(["track"]);
      button.click();
      await settle();
      await settle();
      expect(button.textContent).toBe("Mic on");
      // The agent closes the channel, for example at a lock: the device goes back, and the button goes.
      server.push(closeFrame(mic, { reason: "cancelled" }));
      await settle();
      expect(stopped).toEqual(["track", "track"]);
      expect(host.querySelector(".panel-mic")).toBeNull();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  test("a session with no microphone source, and a browser with no AudioEncoder, show no button", async () => {
    const { host, server, channelOf } = await mountDesktop();
    const mic = channelOf("mic");
    server.push(readyFrame(mic));
    server.push(encodeJsonFrame(mic, FrameType.Control, ready));
    await settle();
    expect(host.querySelector(".panel-mic")).toBeNull();
    stubMicrophone(async () => ({ getTracks: () => [] }));
    try {
      server.push(encodeJsonFrame(mic, FrameType.Control, { type: "unavailable", message: "The session has no microphone source." }));
      await settle();
      expect(host.querySelector(".panel-mic")).toBeNull();
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("camera for the apps", () => {
  function stubCamera(getUserMedia: () => Promise<unknown>, supported = true) {
    const encoded: { frame: { init: { visibleRect: DOMRectInit } }; options: { keyFrame: boolean } }[] = [];
    const encoders: { output: (chunk: unknown) => void; config?: unknown; state: string }[] = [];
    const callbacks: ((now: number) => void)[] = [];
    class FakeEncoder {
      state = "unconfigured";
      encodeQueueSize = 0;
      static isConfigSupported = async () => ({ supported });
      constructor(readonly init: { output: (chunk: unknown) => void }) {
        encoders.push(this as never);
      }
      get output() {
        return this.init.output;
      }
      configure(config: unknown) {
        (this as { config?: unknown }).config = config;
        this.state = "configured";
      }
      encode(frame: never, options: { keyFrame: boolean }) {
        encoded.push({ frame, options });
      }
      close() {
        this.state = "closed";
      }
    }
    class FakeFrame {
      constructor(
        readonly source: unknown,
        readonly init: { visibleRect: DOMRectInit },
      ) {}
      close() {}
    }
    vi.stubGlobal("VideoEncoder", FakeEncoder);
    vi.stubGlobal("VideoFrame", FakeFrame);
    const proto = HTMLVideoElement.prototype as unknown as Record<string, unknown>;
    const saved = { play: proto.play, rvfc: proto.requestVideoFrameCallback };
    proto.play = async () => {};
    proto.requestVideoFrameCallback = (callback: (now: number) => void) => callbacks.push(callback);
    Object.defineProperty(HTMLVideoElement.prototype, "videoWidth", { configurable: true, get: () => 640 });
    Object.defineProperty(HTMLVideoElement.prototype, "videoHeight", { configurable: true, get: () => 480 });
    const media = vi.fn(getUserMedia);
    vi.stubGlobal("navigator", { ...navigator, mediaDevices: { getUserMedia: media } });
    const restore = () => {
      proto.play = saved.play;
      if (saved.rvfc === undefined) delete proto.requestVideoFrameCallback;
      else proto.requestVideoFrameCallback = saved.rvfc;
      delete (HTMLVideoElement.prototype as { videoWidth?: number }).videoWidth;
      delete (HTMLVideoElement.prototype as { videoHeight?: number }).videoHeight;
      vi.unstubAllGlobals();
    };
    return { encoded, encoders, callbacks, media, restore };
  }

  const ready = { type: "ready", codec: "h264", width: 1280, height: 720, frame_rate: 30 };

  test("the button arms the camera, a reading host app starts the capture, and the pictures go as H.264", async () => {
    const stopped: string[] = [];
    const track = { stop: () => stopped.push("track"), addEventListener: () => {} };
    const { encoded, encoders, callbacks, media, restore } = stubCamera(async () => ({ getTracks: () => [track] }));
    try {
      const { host, server, channelOf } = await mountDesktop();
      const cam = channelOf("camera");
      server.push(readyFrame(cam));
      expect(host.querySelector(".panel-camera")).toBeNull();
      server.push(encodeJsonFrame(cam, FrameType.Control, ready));
      await settle();
      const button = host.querySelector<HTMLButtonElement>(".panel-camera")!;
      expect(button.textContent).toBe("Cam off");
      server.take();
      const sent = () => server.take().filter((f) => f.channel === cam);
      button.click();
      await settle();
      expect(sent().map((f) => parseJson(f))).toEqual([{ type: "arm" }]);
      expect(button.textContent).toBe("Cam ready");
      expect(button.getAttribute("aria-pressed")).toBe("true");
      expect(media).not.toHaveBeenCalled();
      // An app of the host opens the camera.
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "reading" }));
      await settle();
      await settle();
      expect(media).toHaveBeenCalledWith({ video: { width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30, max: 30 } } });
      expect(encoders[0]!.config).toEqual({ codec: "avc1.42001f", width: 1280, height: 720, bitrate: 1_500_000, framerate: 30, latencyMode: "realtime", avc: { format: "annexb" } });
      expect(sent().map((f) => parseJson(f))).toEqual([{ type: "format", codec: "h264", width: 1280, height: 720 }]);
      expect(button.textContent).toBe("Cam on");
      expect(button.dataset.state).toBe("on");
      // A picture of 640x480 loses its top and bottom for 16:9. The first one is a key frame.
      callbacks.shift()!(1000);
      callbacks.shift()!(1010);
      callbacks.shift()!(1040);
      expect(encoded.map((e) => e.options.keyFrame)).toEqual([true, false]);
      expect(encoded[0]!.frame.init.visibleRect).toEqual({ x: 0, y: 60, width: 640, height: 360 });
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "key-frame" }));
      await settle();
      callbacks.shift()!(1080);
      expect(encoded.map((e) => e.options.keyFrame)).toEqual([true, false, true]);
      const chunk = (type: string) => ({ type, byteLength: 3, copyTo: (to: Uint8Array) => to.set([0, 0, 1]) });
      encoders[0]!.output(chunk("key"));
      encoders[0]!.output(chunk("delta"));
      const pictures = sent();
      expect(pictures.map((f) => f.type)).toEqual([FrameType.Data, FrameType.Data]);
      expect([...pictures[0]!.payload]).toEqual([1, 0, 0, 1]);
      expect([...pictures[1]!.payload]).toEqual([0, 0, 0, 1]);
      // The app of the host closes the camera: the device goes back, and the camera stays armed.
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "idle" }));
      await settle();
      expect(stopped).toEqual(["track"]);
      expect(encoders[0]!.state).toBe("closed");
      expect(button.textContent).toBe("Cam ready");
      button.click();
      await settle();
      expect(sent().map((f) => parseJson(f))).toEqual([{ type: "disarm" }]);
      expect(button.textContent).toBe("Cam off");
    } finally {
      restore();
    }
  });

  test("a refusal of the browser shows, a different connection takes the camera, and a close releases it", async () => {
    let refuse = true;
    const stopped: string[] = [];
    const { media, restore } = stubCamera(async () => {
      if (refuse) throw new DOMException("Permission denied", "NotAllowedError");
      return { getTracks: () => [{ stop: () => stopped.push("track"), addEventListener: () => {} }] };
    });
    try {
      const { host, server, channelOf } = await mountDesktop();
      const cam = channelOf("camera");
      server.push(readyFrame(cam));
      server.push(encodeJsonFrame(cam, FrameType.Control, ready));
      await settle();
      const button = host.querySelector<HTMLButtonElement>(".panel-camera")!;
      button.click();
      await settle();
      server.take();
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "reading" }));
      await settle();
      expect(button.textContent).toBe("Cam blocked");
      expect(button.title).toContain("site settings");
      expect(server.take().filter((f) => f.channel === cam).map((f) => parseJson(f))).toEqual([{ type: "disarm" }]);
      refuse = false;
      button.click();
      await settle();
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "reading" }));
      await settle();
      await settle();
      expect(media).toHaveBeenCalledTimes(2);
      expect(button.textContent).toBe("Cam on");
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "taken" }));
      await settle();
      expect(button.textContent).toBe("Cam off");
      expect(stopped).toEqual(["track"]);
      button.click();
      await settle();
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "reading" }));
      await settle();
      await settle();
      expect(button.textContent).toBe("Cam on");
      // The agent closes the channel, for example at a lock: the device goes back, and the button goes.
      server.push(closeFrame(cam, { reason: "cancelled" }));
      await settle();
      expect(stopped).toEqual(["track", "track"]);
      expect(host.querySelector(".panel-camera")).toBeNull();
    } finally {
      restore();
    }
  });

  test("a block of the host shows its cause, and no camera node or no VideoEncoder show no button", async () => {
    const { host, server, channelOf } = await mountDesktop();
    const cam = channelOf("camera");
    server.push(readyFrame(cam));
    server.push(encodeJsonFrame(cam, FrameType.Control, ready));
    await settle();
    expect(host.querySelector(".panel-camera")).toBeNull();
    const { restore } = stubCamera(async () => ({ getTracks: () => [] }));
    try {
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "unavailable", message: "The session has no camera node." }));
      await settle();
      expect(host.querySelector(".panel-camera")).toBeNull();
      server.push(encodeJsonFrame(cam, FrameType.Control, { type: "blocked", message: "The camera switch of the privacy settings of the host is off." }));
      await settle();
      const button = host.querySelector<HTMLButtonElement>(".panel-camera")!;
      expect(button.textContent).toBe("Cam disabled");
      expect(button.title).toContain("privacy settings");
      expect(button.getAttribute("aria-disabled")).toBe("true");
      server.take();
      button.click();
      await settle();
      expect(server.take().filter((f) => f.channel === cam)).toEqual([]);
    } finally {
      restore();
    }
  });
});

describe("apps of the host", () => {
  test("the launcher lists the desktop entries with their icons, and a click starts the app", async () => {
    URL.createObjectURL = vi.fn(() => "blob:calculator-icon");
    URL.revokeObjectURL = vi.fn();
    const { host, server, channelOf } = await mountDesktop();
    const apps = channelOf("apps");
    const requests = () =>
      server
        .take()
        .filter((f) => f.channel === apps)
        .map((f) => parseJson(f));
    const event = (message: object) => server.push(encodeJsonFrame(apps, FrameType.Control, message));
    server.push(readyFrame(apps));
    event({
      type: "list",
      apps: [{ id: "org.gnome.Calculator", name: "Calculator", comment: "Perform calculations", keywords: ["math"], icon: true, names: ["org.gnome.calculator"] }],
    });
    await settle();
    byText(host, "button", "Apps").click();
    await settle();
    expect(requests()).toEqual([{ type: "list" }, { type: "icon", id: "org.gnome.Calculator" }]);
    const option = () => byText(host, "#launcher [role=option]", "Calculator");
    expect(option().querySelector(".launcher-description")?.textContent).toBe("Perform calculations");
    expect(option().dataset.host).toBe("true");
    event({ type: "icon", id: "org.gnome.Calculator", mime: "image/svg+xml" });
    server.push(dataFrame(apps, new TextEncoder().encode("<svg/>")));
    await settle();
    expect(option().querySelector("img")?.getAttribute("src")).toBe("blob:calculator-icon");

    // A start that fails: the launcher stays open and tells why.
    option().click();
    await settle();
    expect(requests()).toEqual([{ type: "launch", id: "org.gnome.Calculator" }]);
    event({ type: "failed", id: "org.gnome.Calculator", message: "Calculator did not start: no such file." });
    await settle();
    expect(host.querySelector("#launcher [role=alert]")?.textContent).toBe("Calculator did not start: no such file.");

    // A start that works: the launcher closes. The window comes from the compositor.
    option().click();
    await settle();
    expect(requests()).toEqual([{ type: "launch", id: "org.gnome.Calculator" }]);
    event({ type: "launched", id: "org.gnome.Calculator" });
    await settle();
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
  });
});

describe("system tray", () => {
  test("the panel shows one button for each item, and a click goes to the program", async () => {
    const { host, server, channelOf } = await mountDesktop();
    const tray = channelOf("tray");
    const requests = () =>
      server
        .take()
        .filter((f) => f.channel === tray)
        .map((f) => parseJson(f));
    // With no item, the panel has no tray.
    expect(host.querySelector(".panel-tray")).toBeNull();
    server.push(readyFrame(tray));
    const item = (id: number, title: string, more: object = {}) => ({ id, name: title.toLowerCase(), title, status: "active", menu: false, menu_only: false, ...more });
    server.push(encodeJsonFrame(tray, FrameType.Control, { type: "items", items: [item(2, "Seafile", { tooltip: "In sync" }), item(1, "Bitwarden", { icon: "AAAA", icon_mime: "image/png" })] }));
    await settle();
    const buttons = () => [...host.querySelectorAll<HTMLButtonElement>(".panel-tray .panel-tray-item")];
    expect(host.querySelector(".panel-tray")?.getAttribute("aria-label")).toBe("System tray");
    expect(buttons().map((button) => button.getAttribute("aria-label"))).toEqual(["Bitwarden", "Seafile"]);
    expect(buttons()[0]!.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AAAA");
    // An item with no icon shows a letter.
    expect(buttons()[1]!.textContent).toBe("S");
    expect(buttons()[1]!.title).toBe("Seafile: In sync");

    requests();
    buttons()[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: 900, clientY: 12 }));
    buttons()[1]!.dispatchEvent(new MouseEvent("auxclick", { bubbles: true, button: 1, clientX: 930, clientY: 12 }));
    await settle();
    expect(requests()).toEqual([
      { type: "activate", id: 1, x: 900, y: 12 },
      { type: "secondary-activate", id: 2, x: 930, y: 12 },
    ]);

    server.push(encodeJsonFrame(tray, FrameType.Control, { type: "item", item: item(2, "Seafile", { status: "needs-attention" }) }));
    await settle();
    expect(buttons()[1]!.dataset.status).toBe("needs-attention");
    server.push(encodeJsonFrame(tray, FrameType.Control, { type: "removed", id: 1 }));
    server.push(encodeJsonFrame(tray, FrameType.Control, { type: "removed", id: 2 }));
    await settle();
    expect(host.querySelector(".panel-tray")).toBeNull();
  });
});

describe("bookmarks in the launcher", () => {
  test("the launcher shows the bookmarks only with the option, and opens them on the host or in a tab", async () => {
    const opened = vi.spyOn(window, "open").mockReturnValue(null);
    try {
      const { host, server, channelOf } = await mountDesktop();
      const marks = channelOf("bookmarks");
      const requests = () =>
        server
          .take()
          .filter((f) => f.channel === marks)
          .map((f) => parseJson(f));
      const loaded = (launcher: boolean) =>
        server.push(
          encodeJsonFrame(marks, FrameType.Control, {
            type: "loaded",
            path: "/home/alice/.config/farglass/bookmarks.toml",
            file: {
              launcher,
              bookmarks: [
                { name: "gh", url: "https://github.com/", browser: "firefox", profile: "work", private: true },
                { name: "docs", url: "https://docs.example/", browser: "client" },
              ],
            },
            browsers: [{ id: "firefox", name: "Firefox", profiles: ["work"] }],
            problems: [],
          }),
        );
      server.push(readyFrame(marks));
      loaded(false);
      await settle();
      const input = () => host.querySelector<HTMLInputElement>(".launcher-search")!;
      byText(host, "button", "Apps").click();
      await settle();
      type(input(), "gh");
      await settle();
      // The option is off by default: no bookmark in the results.
      const titles = () => [...host.querySelectorAll("#launcher [role=option] .launcher-title")].map((el) => el.textContent);
      expect(titles()).not.toContain("gh");
      expect(requests()).toContainEqual({ type: "load" });

      loaded(true);
      await settle();
      type(input(), "gh");
      await settle();
      const option = byText(host, "#launcher [role=option]", "gh");
      expect(option.querySelector(".launcher-description")?.textContent).toBe("Firefox · work · private · github.com");
      requests();
      key("keydown", { key: "Enter" }, input());
      await settle();
      expect(requests()).toEqual([{ type: "open", name: "gh" }]);
      server.push(encodeJsonFrame(marks, FrameType.Control, { type: "opened", name: "gh" }));
      await settle();
      expect(host.querySelector("#launcher [role=option]")).toBeNull();

      // A bookmark of the client browser opens a tab here, and the agent gets no request.
      byText(host, "button", "Apps").click();
      await settle();
      type(input(), "docs");
      await settle();
      key("keydown", { key: "Enter" }, input());
      await settle();
      expect(opened).toHaveBeenCalledWith("https://docs.example/", "_blank", "noopener,noreferrer");
      expect(requests().filter((r) => (r as { type: string }).type === "open")).toEqual([]);
    } finally {
      opened.mockRestore();
    }
  });
});

describe("bookmarks app", () => {
  test("an agent that refuses the channel gives a message, not Loading forever", async () => {
    const { host, server, channelOf } = await mountDesktop();
    server.push(closeFrame(channelOf("bookmarks"), { reason: "not-supported", message: "The agent does not know this channel kind." }));
    await settle();
    byText(host, "button", "Apps").click();
    await settle();
    byText(host, "#launcher [role=option]", "Bookmarks").click();
    await vi.waitFor(() => expect(host.querySelector(".bookmarks [role=alert]")).not.toBeNull());
    expect(host.querySelector(".bookmarks [role=alert]")?.textContent).toContain("older than the shell");
    expect(host.querySelector(".bookmarks-empty[role=status]")).toBeNull();
  });

  test("the app lists the bookmarks with their problems and saves a new one", async () => {
    const { host, server, channelOf } = await mountDesktop();
    const marks = channelOf("bookmarks");
    const requests = () =>
      server
        .take()
        .filter((f) => f.channel === marks)
        .map((f) => parseJson(f));
    const file = { launcher: false, bookmarks: [{ name: "gh", url: "https://github.com/", browser: "brave" }] };
    const loaded = (current: typeof file, problems: { index: number; message: string }[] = []) =>
      server.push(
        encodeJsonFrame(marks, FrameType.Control, {
          type: "loaded",
          path: "/home/alice/.config/farglass/bookmarks.toml",
          file: current,
          browsers: [{ id: "firefox", name: "Firefox", profiles: ["default-release", "work"] }],
          problems,
        }),
      );
    server.push(readyFrame(marks));
    loaded(file, [{ index: 0, message: "brave is not installed on this host." }]);
    byText(host, "button", "Apps").click();
    await settle();
    byText(host, "#launcher [role=option]", "Bookmarks").click();
    await settle();
    await settle();
    await vi.waitFor(() => expect(host.querySelector(".bookmarks-row")).not.toBeNull());
    const row = byText(host, ".bookmarks-row", "gh");
    expect(row.querySelector(".bookmarks-problem")?.textContent).toBe("brave is not installed on this host.");
    expect(row.querySelector<HTMLButtonElement>(".bookmarks-open")!.disabled).toBe(true);
    requests();

    byText(host, ".bookmarks button", "Add").click();
    await settle();
    const field = (id: string) => host.querySelector<HTMLInputElement & HTMLSelectElement>(`#${id}`)!;
    type(field("bookmark-name"), "mail");
    type(field("bookmark-url"), "https://mail.example/");
    field("bookmark-browser").value = "firefox";
    field("bookmark-browser").dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    field("bookmark-profile").value = "work";
    field("bookmark-profile").dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    const form = host.querySelector<HTMLFormElement>(".bookmarks-form")!;
    form.requestSubmit();
    await settle();
    const saved = { launcher: false, bookmarks: [...file.bookmarks, { name: "mail", url: "https://mail.example/", browser: "firefox", profile: "work" }] };
    expect(requests()).toEqual([{ type: "save", file: saved }]);
    loaded(saved as typeof file);
    await settle();
    expect(host.querySelector(".bookmarks-form")).toBeNull();
    expect(byText(host, ".bookmarks-row", "mail").querySelector(".bookmarks-line")?.textContent).toBe("Firefox · work · mail.example");
  });
});

describe("launcher", () => {
  test("the Apps button opens the launcher with all apps, and a click opens an app", async () => {
    const { host } = await mountDesktop();
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
    byText(host, "button", "Apps").click();
    await settle();
    const options = [...host.querySelectorAll("#launcher [role=option]")].map((li) => li.querySelector(".launcher-title")?.textContent);
    expect(options).toEqual(["About farglass", "Bookmarks", "Echo", "Network check", "Settings", "Terminal"]);
    byText(host, "#launcher [role=option]", "About farglass").click();
    await settle();
    expect(host.querySelector(".wm-window")?.getAttribute("aria-label")).toBe("About farglass");
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
  });

  test("the search filters the apps, and Enter opens the selected app", async () => {
    const { host } = await mountDesktop();
    byText(host, "button", "Apps").click();
    await settle();
    const input = host.querySelector<HTMLInputElement>(".launcher-search")!;
    type(input, "ping");
    await settle();
    expect([...host.querySelectorAll("#launcher [role=option]")].map((li) => li.querySelector(".launcher-title")?.textContent)).toEqual(["Echo", "Network check"]);
    type(input, "nothing here");
    await settle();
    expect(host.querySelector(".launcher-empty")?.textContent).toBe("No app matches.");
    type(input, "");
    await settle();
    key("keydown", { key: "ArrowDown" }, input);
    await settle();
    key("keydown", { key: "ArrowDown" }, input);
    await settle();
    expect(host.querySelector("#launcher [aria-selected=true] .launcher-title")?.textContent).toBe("Echo");
    key("keydown", { key: "Enter" }, input);
    await settle();
    expect(host.querySelector(".wm-window")?.getAttribute("aria-label")).toBe("Echo");
  });

  test("a search finds a settings page by a keyword, and Enter opens the settings at that page", async () => {
    const { host, server } = await mountDesktop();
    byText(host, "button", "Apps").click();
    await settle();
    const input = host.querySelector<HTMLInputElement>(".launcher-search")!;
    type(input, "timezone");
    await settle();
    const option = host.querySelector("#launcher [aria-selected=true]")!;
    // The settings are one entry (F076): it tells the page that it opens.
    expect([option.querySelector(".launcher-title")?.textContent, option.querySelector(".launcher-description")?.textContent]).toEqual(["Settings", "Opens the page Date and time"]);
    expect(host.querySelectorAll("#launcher [role=option]")).toHaveLength(1);
    server.take();
    key("keydown", { key: "Enter" }, input);
    await settle();
    const win = host.querySelector(".wm-window")!;
    expect(win.getAttribute("aria-label")).toBe("Settings");
    const sets = server
      .take()
      .filter((frame) => frame.type === FrameType.Control)
      .map((frame) => parseJson<{ type: string; key?: string; value?: unknown }>(frame))
      .filter((message) => message.type === "set" && message.key?.startsWith("settings.page."));
    expect(sets.map((message) => message.value)).toEqual(["datetime"]);
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
  });

  test("a pinned app and a used app come first, in sections, and the state store gets them", async () => {
    const { host, server } = await mountDesktop();
    const titles = () => [...host.querySelectorAll("#launcher [role=option]")].map((li) => li.querySelector(".launcher-title")?.textContent);
    const sections = () => [...host.querySelectorAll("#launcher .launcher-section")].map((li) => li.textContent?.trim());
    byText(host, "button", "Apps").click();
    await settle();
    expect(sections()).toEqual([]);
    const input = host.querySelector<HTMLInputElement>(".launcher-search")!;
    key("keydown", { key: "ArrowDown" }, input);
    await settle();
    key("keydown", { key: "ArrowDown" }, input);
    await settle();
    server.take();
    // Alt+P pins the selected app, Echo. The selection stays on it.
    key("keydown", { key: "p", code: "KeyP", altKey: true }, input);
    await settle();
    expect([sections(), titles()]).toEqual([["Pinned", "All apps"], ["Echo", "About farglass", "Bookmarks", "Network check", "Settings", "Terminal"]]);
    expect(host.querySelector("#launcher [aria-selected=true] .launcher-title")?.textContent).toBe("Echo");
    const sets = () =>
      server
        .take()
        .filter((frame) => frame.type === FrameType.Control)
        .map((frame) => parseJson<{ type: string; key?: string; value?: { pinned: string[]; used: Record<string, unknown> } }>(frame))
        .filter((message) => message.type === "set" && message.key === "launcher");
    expect(sets().map((message) => message.value?.pinned)).toEqual([["echo"]]);
    // A start counts: the app is in the most used apps at the next open.
    byText(host, "#launcher [role=option]", "Terminal").click();
    await settle();
    expect(Object.keys(sets().at(-1)?.value?.used ?? {})).toEqual(["terminal"]);
    byText(host, "button", "Apps").click();
    await settle();
    expect([sections(), titles()]).toEqual([["Pinned", "Most used", "All apps"], ["Echo", "Terminal", "About farglass", "Bookmarks", "Network check", "Settings"]]);
    // The button of the entry removes the pin.
    byText(host, "#launcher [role=option]", "Echo").querySelector<HTMLButtonElement>(".launcher-pin")!.click();
    await settle();
    expect([sections(), titles()]).toEqual([["Most used", "All apps"], ["Terminal", "About farglass", "Bookmarks", "Echo", "Network check", "Settings"]]);
  });

  test("a calculation and a unit conversion are the first result, and Enter copies the value", async () => {
    const writes: string[] = [];
    Object.defineProperty(navigator, "clipboard", { value: { writeText: async (text: string) => void writes.push(text) }, configurable: true });
    const { host } = await mountDesktop();
    byText(host, "button", "Apps").click();
    await settle();
    const input = host.querySelector<HTMLInputElement>(".launcher-search")!;
    type(input, "2 * (3 + 4)^2");
    await settle();
    expect(host.querySelector("#launcher [aria-selected=true] .launcher-title")?.textContent).toBe("2 * (3 + 4)^2 = 98");
    type(input, "10 km in mi");
    await settle();
    expect(host.querySelector("#launcher [aria-selected=true] .launcher-title")?.textContent).toBe("10 km = 6.21371192237 mi");
    key("keydown", { key: "Enter" }, input);
    await settle();
    expect(writes).toEqual(["6.21371192237"]);
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
  });

  test("Alt+F1 and a tap of Super open and close the launcher", async () => {
    const { host } = await mountDesktop();
    const open = () => host.querySelector("#launcher .launcher-search") !== null;
    key("keydown", { key: "F1", code: "F1", altKey: true });
    await settle();
    expect(open()).toBe(true);
    key("keydown", { key: "F1", code: "F1", altKey: true });
    await settle();
    expect(open()).toBe(false);

    key("keydown", { key: "Meta", code: "MetaLeft", metaKey: true });
    key("keyup", { key: "Meta", code: "MetaLeft" });
    await settle();
    expect(open()).toBe(true);
    // Super with a second key is a shortcut, not a tap.
    key("keydown", { key: "Meta", code: "MetaLeft", metaKey: true });
    key("keydown", { key: "ArrowUp", code: "ArrowUp", metaKey: true });
    key("keyup", { key: "Meta", code: "MetaLeft" });
    await settle();
    expect(open()).toBe(true);
  });
});

describe("session menu", () => {
  test("Lock sends the lock message, and Log out logs out", async () => {
    let logouts = 0;
    const { host, server } = await mountDesktop(() => (logouts += 1));
    const session = byText(host, ".panel-session", "alice");
    expect(session.textContent).toBe("Logged in as alice");
    session.click();
    await settle();
    server.take();
    byText(host, "[role=menuitem]", "Lock").click();
    expect(server.take().map((frame) => parseJson(frame))).toEqual([{ type: "lock" }]);
    expect(logouts).toBe(0);
    byText(host, "[role=menuitem]", "Log out").click();
    expect(logouts).toBe(1);
  });
});

describe("clock", () => {
  test("the clock shows the time and changes at the next minute", async () => {
    vi.useFakeTimers();
    let now = new Date(2026, 10, 19, 13, 59, 30);
    const host = document.createElement("div");
    document.body.append(host);
    dispose = render(() => <Clock now={() => now} />, host);
    const time = host.querySelector("time")!;
    const before = time.textContent;
    expect(before).toMatch(/13.?59|1:59/);
    now = new Date(2026, 10, 19, 14, 0, 1);
    await vi.advanceTimersByTimeAsync(31_000);
    expect(time.textContent).not.toBe(before);
    expect(time.getAttribute("datetime")).toBe(now.toISOString());
  });
});
