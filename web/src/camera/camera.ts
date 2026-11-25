/**
 * The camera of this computer for the apps of the host (F116, ADR-056, ADR-057).
 *
 * The camera keeps a camera channel open while the desktop shows, and stays
 * off until the user arms it in the panel. While it is armed, the shell
 * captures only when the agent says that an app of the host reads the camera,
 * thus the camera light of this computer is on only while an app uses it.
 *
 * The shell crops each picture to the aspect of the format, and WebCodecs
 * scales it and encodes it as H.264 in real time. Each data frame holds one
 * picture: a flags byte (bit 0 is a key frame), then the access unit in
 * Annex B form. A late picture is dropped, not queued: while the send buffer
 * is above a limit, the pictures wait for the next key frame.
 */

import { createSignal } from "solid-js";
import type { CameraEvent, CameraRequest, Channel, Connection } from "../proto";

/** The state of the camera: off, armed, on (an app of the host gets the pictures), refused by the browser, or blocked by the host. */
export type CameraState = "off" | "armed" | "on" | "refused" | "blocked";

type Format = Extract<CameraEvent, { type: "ready" }>;

/** The bit rate of the pictures: good quality for a video conference. */
const BITRATE = 1_500_000;

/** H.264 Constrained Baseline, level 3.1: 1280x720 at 30 pictures each second. */
const CODEC = "avc1.42001f";

/** The bytes that can wait to go out. Above it, the shell drops pictures, so the camera does not delay the input. */
const SEND_LIMIT = 256 * 1024;

/** The pictures that can wait in the encoder. Above it, a picture is late and goes. */
const ENCODE_QUEUE = 2;

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

/** Returns the part of a picture of `width`x`height` with the aspect of `format`, centered, with even values. */
export function cropRect(width: number, height: number, format: { width: number; height: number }): DOMRectInit {
  const even = (value: number) => Math.floor(value / 2) * 2;
  if (width * format.height > height * format.width) {
    const cropped = even((height * format.width) / format.height);
    return { x: even((width - cropped) / 2), y: 0, width: cropped, height: even(height) };
  }
  const cropped = even((width * format.height) / format.width);
  return { x: 0, y: even((height - cropped) / 2), width: even(width), height: cropped };
}

export class Camera {
  readonly #channel: Channel;
  #format: Format | null = null;
  #stream: MediaStream | null = null;
  #video: HTMLVideoElement | null = null;
  #encoder: VideoEncoder | null = null;
  /** The next picture must be a key frame. */
  #needKey = true;
  /** The shell dropped a picture: the pictures after it wait for a key frame. */
  #waitKey = false;
  #lastFrame = 0;
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
        this.#needKey = true;
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
    const config: VideoEncoderConfig = {
      codec: CODEC,
      width: format.width,
      height: format.height,
      bitrate: BITRATE,
      framerate: format.frame_rate,
      latencyMode: "realtime",
      avc: { format: "annexb" },
    };
    const support = await VideoEncoder.isConfigSupported(config).catch(() => ({ supported: false }));
    if (!support.supported) {
      console.warn("This browser cannot encode the camera as H.264.");
      release();
      if (current()) this.disarm();
      return;
    }
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.srcObject = stream;
    try {
      await video.play();
    } catch (error) {
      console.error("camera", error);
      release();
      if (current()) this.disarm();
      return;
    }
    if (!current()) {
      release();
      return;
    }
    const encoder = new VideoEncoder({ output: (chunk) => this.#send(chunk), error: (error) => console.error("camera", error) });
    encoder.configure(config);
    this.#stream = stream;
    this.#video = video;
    this.#encoder = encoder;
    this.#needKey = true;
    this.#waitKey = false;
    this.#lastFrame = 0;
    this.#channel.control({ type: "format", codec: format.codec, width: format.width, height: format.height } satisfies CameraRequest);
    this.#state[1]("on");
    // An unplugged device, or a block in the site settings, ends the track.
    for (const track of stream.getTracks()) {
      track.addEventListener("ended", () => {
        if (this.#stream === stream) this.disarm();
      });
    }
    const step = (now: number) => {
      if (this.#video !== video) return;
      this.#encode(video, format, now);
      video.requestVideoFrameCallback(step);
    };
    video.requestVideoFrameCallback(step);
  }

  /** Encodes the picture that `video` shows now, unless it is too early or the encoder is late. */
  #encode(video: HTMLVideoElement, format: Format, now: number): void {
    const encoder = this.#encoder;
    if (!encoder || encoder.state !== "configured" || !video.videoWidth || !video.videoHeight) return;
    // A camera that gives more pictures than the format: keep the frame rate (2 ms for the jitter of the callbacks).
    if (this.#lastFrame && now - this.#lastFrame < 1000 / format.frame_rate - 2) return;
    if (encoder.encodeQueueSize > ENCODE_QUEUE) {
      this.#needKey = true;
      return;
    }
    this.#lastFrame = now;
    const frame = new VideoFrame(video, {
      timestamp: Math.round(now * 1000),
      visibleRect: cropRect(video.videoWidth, video.videoHeight, format),
    });
    encoder.encode(frame, { keyFrame: this.#needKey });
    this.#needKey = false;
    frame.close();
  }

  #send(chunk: EncodedVideoChunk): void {
    const key = chunk.type === "key";
    if (this.#waitKey && !key) return;
    if (this.#channel.buffered > SEND_LIMIT) {
      // A late picture: drop it, and the pictures after it until a key frame.
      this.#waitKey = true;
      this.#needKey = true;
      return;
    }
    this.#waitKey = false;
    const bytes = new Uint8Array(1 + chunk.byteLength);
    bytes[0] = key ? 1 : 0;
    chunk.copyTo(bytes.subarray(1));
    this.#channel.send(bytes);
  }

  #stopCapture(): void {
    this.#generation++;
    for (const track of this.#stream?.getTracks() ?? []) track.stop();
    if (this.#encoder && this.#encoder.state !== "closed") this.#encoder.close();
    if (this.#video) this.#video.srcObject = null;
    this.#stream = null;
    this.#video = null;
    this.#encoder = null;
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
