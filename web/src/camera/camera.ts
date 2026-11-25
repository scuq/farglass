/**
 * The camera of this computer for the apps of the host (F116, F118, ADR-056, ADR-057, ADR-058).
 *
 * The camera keeps a camera channel open while the desktop shows, and stays
 * off until the user arms it in the panel. While it is armed, the shell
 * captures only when the agent says that an app of the host reads the camera,
 * thus the camera light of this computer is on only while an app uses it.
 *
 * The camera must not delay the desktop. It encodes in a Worker where the
 * browser has MediaStreamTrackProcessor, else on the main thread. The agent
 * acks each picture, and a new picture goes to the encoder only after the
 * ack (see encoder.ts and rate.ts). Each data frame holds one picture: a
 * flags byte (bit 0 is a key frame), then the access unit in Annex B form.
 */

import { createSignal } from "solid-js";
import type { CameraEvent, CameraRequest, Channel, Connection } from "../proto";
import { CameraEncoder, encoderConfig, type EncodedPicture } from "./encoder";
import type { FromWorker, ToWorker } from "./worker";

/** The state of the camera: off, armed, on (an app of the host gets the pictures), refused by the browser, or blocked by the host. */
export type CameraState = "off" | "armed" | "on" | "refused" | "blocked";

type Format = Extract<CameraEvent, { type: "ready" }>;

/** The bytes that can wait in the transport before a new picture: on a WebSocket, for all channels. */
const ROOM_LIMIT = 16 * 1024;

/** The bytes that can wait when a picture goes out. Above it, the picture goes, so the camera does not delay the input. */
const SEND_LIMIT = 256 * 1024;

/** The time between two checks of the room of the transport for the worker, in ms. */
const ROOM_CHECK_MS = 20;

type Processor = new (init: { track: MediaStreamTrack }) => { readable: ReadableStream<VideoFrame> };

/** Returns MediaStreamTrackProcessor, if this browser has it. */
function trackProcessor(): Processor | undefined {
  return (globalThis as { MediaStreamTrackProcessor?: Processor }).MediaStreamTrackProcessor;
}

/** Reports whether this browser can capture and encode the camera. */
export function cameraSupported(): boolean {
  return (
    typeof VideoEncoder !== "undefined" &&
    typeof VideoFrame !== "undefined" &&
    typeof navigator.mediaDevices?.getUserMedia === "function" &&
    typeof HTMLVideoElement !== "undefined" &&
    "requestVideoFrameCallback" in HTMLVideoElement.prototype
  );
}

export { cropRect } from "./encoder";

export class Camera {
  readonly #channel: Channel;
  #format: Format | null = null;
  #stream: MediaStream | null = null;
  /** The main thread path: the video element and the encoder. */
  #video: HTMLVideoElement | null = null;
  #encoder: CameraEncoder | null = null;
  /** The Worker path, and the check of the room of the transport for it. */
  #worker: Worker | null = null;
  #roomCheck: ReturnType<typeof setInterval> | undefined;
  readonly #rate = createSignal({ bitrate: 0, frameRate: 0 });
  // Each capture and each stop counts up: a capture that a stop overtook releases its device.
  #generation = 0;
  readonly #available = createSignal(false);
  readonly #state = createSignal<CameraState>("off");
  readonly #message = createSignal("");

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "camera" },
      {
        oncontrol: (message) => this.#event(message as CameraEvent),
        // A refusal, a lock, a logout, or a lost connection: the device goes back to the browser.
        onclose: () => {
          this.#available[1](false);
          this.#stopCapture();
          this.#state[1]("off");
        },
      },
    );
  }

  /** Reports whether the panel shows the camera button: the session has a camera node, or the host blocks it. */
  available(): boolean {
    return this.#available[0]();
  }

  state(): CameraState {
    return this.#state[0]();
  }

  /** The bit rate and the frame rate of the camera now, for the panel and the tests. */
  rate(): { bitrate: number; frameRate: number } {
    return this.#rate[0]();
  }

  /** The cause that the host gave for a block. */
  message(): string {
    return this.#message[0]();
  }

  /** Arms the camera, or turns it off if it is armed or on. */
  toggle(): void {
    switch (this.#state[0]()) {
      case "off":
      case "refused":
        this.arm();
        break;
      case "armed":
      case "on":
        this.disarm();
        break;
      case "blocked":
        break;
    }
  }

  /** Arms the camera: the capture starts when an app of the host reads it. */
  arm(): void {
    if (!this.#format || !this.#channel.open) return;
    this.#channel.control({ type: "arm" } satisfies CameraRequest);
    this.#state[1]("armed");
  }

  /** Turns the camera off and gives the device back to the browser. */
  disarm(): void {
    const state = this.#state[0]();
    if (state === "armed" || state === "on") this.#channel.control({ type: "disarm" } satisfies CameraRequest);
    this.#stopCapture();
    this.#state[1]("off");
  }

  close(): void {
    this.#stopCapture();
    this.#channel.close();
  }

  #event(event: CameraEvent): void {
    switch (event.type) {
      case "ready":
        this.#format = event;
        this.#available[1](cameraSupported());
        break;
      case "unavailable":
        this.#format = null;
        this.#available[1](false);
        break;
      case "blocked":
        this.#format = null;
        this.#message[1](event.message);
        this.#state[1]("blocked");
        this.#available[1](true);
        break;
      case "reading":
        if (this.#state[0]() === "armed") void this.#capture();
        break;
      case "idle":
        if (this.#state[0]() === "on") {
          this.#stopCapture();
          this.#state[1]("armed");
        }
        break;
      case "key-frame":
        this.#encoder?.keyFrame();
        this.#post({ type: "key-frame" });
        break;
      case "ack":
        this.#encoder?.ack();
        if (this.#encoder) this.#rate[1](this.#encoder.rate);
        this.#post({ type: "ack" });
        break;
      case "taken":
        // A different tab or computer of the user armed the camera.
        this.#stopCapture();
        this.#state[1]("off");
        break;
    }
  }

  /** Opens the camera and sends its pictures to the host. */
  async #capture(): Promise<void> {
    const format = this.#format;
    if (!format) return;
    const generation = ++this.#generation;
    const current = () => generation === this.#generation && this.#channel.open && this.#state[0]() === "armed";
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        video: { width: { ideal: format.width }, height: { ideal: format.height }, frameRate: { ideal: format.frame_rate, max: format.frame_rate } },
      });
    } catch (error) {
      console.warn("The browser refused the camera:", error);
      if (current()) {
        this.#channel.control({ type: "disarm" } satisfies CameraRequest);
        this.#state[1]("refused");
      }
      return;
    }
    const release = () => {
      for (const track of stream.getTracks()) track.stop();
    };
    const chosen = await encoderConfig(format);
    if (!chosen) {
      console.warn("This browser cannot encode the camera as H.264.");
      release();
      if (current()) this.disarm();
      return;
    }
    const Processor = trackProcessor();
    const track = Processor ? stream.getVideoTracks()[0] : undefined;
    if (Processor && track) {
      if (!current()) {
        release();
        return;
      }
      this.#startWorker(new Processor({ track }).readable, chosen.config, chosen.software, format);
    } else if (!(await this.#startMainThread(stream, chosen.config, chosen.software, format, current))) {
      release();
      return;
    }
    this.#stream = stream;
    this.#channel.control({ type: "format", codec: format.codec, width: format.width, height: format.height } satisfies CameraRequest);
    this.#state[1]("on");
    // An unplugged device, or a block in the site settings, ends the track.
    for (const each of stream.getTracks()) {
      each.addEventListener("ended", () => {
        if (this.#stream === stream) this.disarm();
      });
    }
  }

  /** Reports whether the transport has room for a new picture. */
  #room(): boolean {
    return this.#channel.buffered < ROOM_LIMIT;
  }

  /** Encodes in a Worker: the main thread only sends the pictures. */
  #startWorker(readable: ReadableStream<VideoFrame>, config: VideoEncoderConfig, software: boolean, format: Format): void {
    const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<FromWorker>) => {
      if (this.#worker !== worker) return;
      const message = event.data;
      if (message.type === "picture") this.#send(message);
      else this.#rate[1]({ bitrate: message.bitrate, frameRate: message.frameRate });
    };
    this.#worker = worker;
    this.#post({ type: "start", readable, config, software, format }, [readable as unknown as Transferable]);
    let room = true;
    this.#roomCheck = setInterval(() => {
      const now = this.#room();
      if (now !== room) {
        room = now;
        this.#post({ type: "sendable", value: now });
      }
    }, ROOM_CHECK_MS);
  }

  /** Encodes on the main thread, for a browser with no MediaStreamTrackProcessor. Returns false if the capture stopped. */
  async #startMainThread(stream: MediaStream, config: VideoEncoderConfig, software: boolean, format: Format, current: () => boolean): Promise<boolean> {
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.srcObject = stream;
    try {
      await video.play();
    } catch (error) {
      console.error("camera", error);
      if (current()) this.disarm();
      return false;
    }
    if (!current()) return false;
    const encoder = new CameraEncoder(config, software, format, (picture) => this.#send(picture));
    this.#video = video;
    this.#encoder = encoder;
    this.#rate[1](encoder.rate);
    const step = (now: number) => {
      if (this.#video !== video) return;
      encoder.offer(video, video.videoWidth, video.videoHeight, this.#room(), now);
      video.requestVideoFrameCallback(step);
    };
    video.requestVideoFrameCallback(step);
    return true;
  }

  #post(message: ToWorker, transfer: Transferable[] = []): void {
    this.#worker?.postMessage(message, transfer);
  }

  #send(picture: EncodedPicture): void {
    if (this.#channel.buffered > SEND_LIMIT) {
      // The transport is far behind: the picture goes, and the next one is a key frame.
      this.#encoder?.dropped();
      this.#post({ type: "dropped" });
      return;
    }
    const bytes = new Uint8Array(1 + picture.data.byteLength);
    bytes[0] = picture.key ? 1 : 0;
    bytes.set(new Uint8Array(picture.data), 1);
    this.#channel.send(bytes);
  }

  #stopCapture(): void {
    this.#generation++;
    for (const track of this.#stream?.getTracks() ?? []) track.stop();
    this.#encoder?.close();
    if (this.#video) this.#video.srcObject = null;
    if (this.#worker) {
      this.#post({ type: "stop" });
      this.#worker.terminate();
    }
    clearInterval(this.#roomCheck);
    this.#stream = null;
    this.#video = null;
    this.#encoder = null;
    this.#worker = null;
  }
}

// A plain variable: the desktop sets it before its panel renders.
let active: Camera | undefined;

/** Returns the camera of the desktop that shows. */
export function camera(): Camera | undefined {
  return active;
}

/** Opens the camera channel for `connection`. Returns the function that closes it and stops the capture. */
export function startCamera(connection: Connection): () => void {
  const cam = new Camera(connection);
  active = cam;
  return () => {
    cam.close();
    if (active === cam) active = undefined;
  };
}
