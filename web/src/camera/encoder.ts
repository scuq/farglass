/**
 * The encoder of the camera, for a Worker and for the main thread (F116, F118, ADR-058).
 *
 * A picture goes to the encoder only when the gate is open:
 * - fewer pictures than the window of the rate wait for their ack,
 * - the frame rate of the rate permits it,
 * - the transport has room (on a WebSocket, less than 16 KiB wait for all channels),
 * - the encoder is not late.
 * The gate closes before the encode, thus a skipped picture breaks no reference of the stream.
 */

import { encoderBitrate, frameRate, initialRate, nextRate, type RateState, START_BITRATE, window } from "./rate";

/** The format of the ready message of the agent. */
export interface CameraFormat {
  readonly codec: string;
  readonly width: number;
  readonly height: number;
  readonly frame_rate: number;
}

/** One encoded picture: the key frame flag and the H.264 access unit in Annex B form. */
export interface EncodedPicture {
  readonly key: boolean;
  readonly data: ArrayBuffer;
}

/** H.264 Constrained Baseline, level 3.1: 1280x720 at 30 pictures each second. */
const CODEC = "avc1.42001f";

/** The pictures that can wait in the encoder. Above it, the encoder is late and the picture goes. */
const ENCODE_QUEUE = 2;

/** A picture with no ack after this time is lost, in ms: it does not hold the window, and the next picture is a key frame. */
const LOST_MS = 1_000;

/** The highest frame rate with a software encoder: the CPU of the client stays free for the desktop. */
const SOFTWARE_FRAME_RATE = 15;

/** The shortest time between two configurations of the encoder for a new bit rate, in ms. */
const CONFIGURE_MS = 1_000;

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

/** Returns the encoder configuration for `format`: a hardware encoder first. Returns null if this browser cannot encode it. */
export async function encoderConfig(format: CameraFormat): Promise<{ config: VideoEncoderConfig; software: boolean } | null> {
  const base: VideoEncoderConfig = {
    codec: CODEC,
    width: format.width,
    height: format.height,
    bitrate: START_BITRATE,
    framerate: format.frame_rate,
    latencyMode: "realtime",
    avc: { format: "annexb" },
  };
  for (const hardwareAcceleration of ["prefer-hardware", "no-preference"] as const) {
    const config = { ...base, hardwareAcceleration };
    const support = await VideoEncoder.isConfigSupported(config).catch(() => ({ supported: false }));
    if (support.supported) return { config, software: hardwareAcceleration !== "prefer-hardware" };
  }
  return null;
}

/** A picture that waits for its ack. */
interface InFlight {
  /** The time of the encode, and then the time that the encoder gave the picture: the round trip starts there. */
  sent: number;
  readonly alone: boolean;
  bits: number | null;
}

export class CameraEncoder {
  readonly #encoder: VideoEncoder;
  readonly #config: VideoEncoderConfig;
  readonly #format: CameraFormat;
  readonly #now: () => number;
  #rate: RateState;
  #bitrate: number;
  #configured = 0;
  #inFlight: InFlight[] = [];
  #needKey = true;
  #last = 0;

  constructor(
    config: VideoEncoderConfig,
    software: boolean,
    format: CameraFormat,
    output: (picture: EncodedPicture) => void,
    now: () => number = () => performance.now(),
  ) {
    this.#config = config;
    this.#format = format;
    this.#now = now;
    this.#rate = initialRate(now(), software ? Math.min(SOFTWARE_FRAME_RATE, format.frame_rate) : format.frame_rate);
    this.#bitrate = config.bitrate ?? START_BITRATE;
    this.#encoder = new VideoEncoder({
      output: (chunk) => {
        const waiting = this.#inFlight.find((picture) => picture.bits === null);
        if (waiting) {
          // The time of the encode is not on the link: it would look like a slow uplink to the rate.
          waiting.bits = chunk.byteLength * 8;
          waiting.sent = this.#now();
        }
        const data = new ArrayBuffer(chunk.byteLength);
        chunk.copyTo(data);
        output({ key: chunk.type === "key", data });
      },
      error: (error) => console.error("camera", error),
    });
    this.#encoder.configure(config);
    this.#configured = now();
  }

  /** The bit rate and the frame rate that the camera uses now. */
  get rate(): { readonly bitrate: number; readonly frameRate: number } {
    return { bitrate: encoderBitrate(this.#rate), frameRate: frameRate(this.#rate) };
  }

  /**
   * Encodes the picture of `source` if the gate is open. `width` and `height` are its size. `sendable` reports
   * whether the transport has room. `at` is the time of the picture, on the clock of `performance.now()`.
   * Returns true if the picture goes to the encoder. The caller closes `source`.
   */
  offer(source: CanvasImageSource | VideoFrame, width: number, height: number, sendable: boolean, at: number = this.#now()): boolean {
    const now = at;
    while (this.#inFlight[0] && now - this.#inFlight[0].sent > LOST_MS) {
      this.#inFlight.shift();
      this.#needKey = true;
    }
    if (this.#encoder.state !== "configured" || !sendable || !width || !height) return false;
    if (this.#inFlight.length >= window(this.#rate)) return false;
    // 2 ms for the jitter of the callbacks of the camera.
    if (this.#last && now - this.#last < 1000 / frameRate(this.#rate) - 2) return false;
    if (this.#encoder.encodeQueueSize > ENCODE_QUEUE) return false;
    this.#last = now;
    const frame = new VideoFrame(source, { timestamp: Math.round(now * 1000), visibleRect: cropRect(width, height, this.#format) });
    this.#inFlight.push({ sent: now, alone: this.#inFlight.length === 0, bits: null });
    this.#encoder.encode(frame, { keyFrame: this.#needKey });
    this.#needKey = false;
    frame.close();
    return true;
  }

  /** The agent acked the oldest picture. */
  ack(): void {
    const picture = this.#inFlight.shift();
    if (!picture || picture.bits === null) return;
    const now = this.#now();
    this.#rate = nextRate(this.#rate, { sent: picture.sent, at: now, bits: picture.bits, alone: picture.alone });
    const bitrate = encoderBitrate(this.#rate);
    if (Math.abs(bitrate - this.#bitrate) >= this.#bitrate * 0.1 && now - this.#configured >= CONFIGURE_MS && this.#encoder.state === "configured") {
      this.#bitrate = bitrate;
      this.#configured = now;
      this.#encoder.configure({ ...this.#config, bitrate });
    }
  }

  /** The newest picture did not go out: it holds no ack, and the next picture is a key frame. */
  dropped(): void {
    const at = this.#inFlight.findLastIndex((picture) => picture.bits !== null);
    if (at >= 0) this.#inFlight.splice(at, 1);
    this.#needKey = true;
  }

  /** The agent asks for a key frame. */
  keyFrame(): void {
    this.#needKey = true;
  }

  close(): void {
    if (this.#encoder.state !== "closed") this.#encoder.close();
  }
}
