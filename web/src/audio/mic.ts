/**
 * The microphone of this computer for the apps of the host (F114).
 *
 * The microphone keeps a mic channel open while the desktop shows, and stays
 * off until the user turns it on in the panel. When it is on, the browser
 * captures with its echo cancellation, an AudioWorklet cuts the samples into
 * blocks of 20 ms, and WebCodecs encodes each block as Opus. Each data frame
 * holds one packet: the packet number as u32, little-endian, then the packet.
 * The last connection of the user that turns the microphone on holds it.
 */

import { createSignal } from "solid-js";
import type { Channel, Connection, MicEvent, MicRequest } from "../proto";
import captureUrl from "./capture.ts?worker&url";

/** The state of the microphone: off, on, or refused by the browser. */
export type MicState = "off" | "on" | "refused";

type Format = Extract<MicEvent, { type: "ready" }>;

/** The bit rate of the packets: good quality for speech. */
const BITRATE = 32_000;

/** Reports whether this browser can capture and encode the microphone. */
export function micSupported(): boolean {
  return (
    typeof AudioEncoder !== "undefined" &&
    typeof AudioData !== "undefined" &&
    typeof AudioContext !== "undefined" &&
    typeof AudioWorkletNode !== "undefined" &&
    typeof navigator.mediaDevices?.getUserMedia === "function"
  );
}

export class Microphone {
  readonly #channel: Channel;
  #format: Format | null = null;
  #stream: MediaStream | null = null;
  #context: AudioContext | null = null;
  #encoder: AudioEncoder | null = null;
  #seq = 0;
  #frames = 0;
  // Each start and stop counts up: a start that a stop overtook releases its device.
  #generation = 0;
  readonly #available = createSignal(false);
  readonly #state = createSignal<MicState>("off");

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "mic" },
      {
        oncontrol: (message) => this.#event(message as MicEvent),
        // A refusal, a lock, a logout, or a lost connection: the device goes back to the browser.
        onclose: () => {
          this.#available[1](false);
          this.#off();
        },
      },
    );
  }

  /** Reports whether the session has a microphone source and this browser can capture. */
  available(): boolean {
    return this.#available[0]();
  }

  state(): MicState {
    return this.#state[0]();
  }

  /** Turns the microphone on, or off if it is on. */
  async toggle(): Promise<void> {
    if (this.#state[0]() === "on") this.stop();
    else await this.start();
  }

  /** Asks the browser for the microphone and sends its sound to the host. */
  async start(): Promise<void> {
    const format = this.#format;
    if (!format || this.#state[0]() === "on") return;
    const generation = ++this.#generation;
    const current = () => generation === this.#generation && this.#channel.open;
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
      });
    } catch (error) {
      console.warn("The browser refused the microphone:", error);
      if (current()) this.#state[1]("refused");
      return;
    }
    const context = new AudioContext({ sampleRate: format.sample_rate, latencyHint: "interactive" });
    const release = () => {
      for (const track of stream.getTracks()) track.stop();
      void context.close().catch(() => {});
    };
    try {
      await context.audioWorklet.addModule(captureUrl);
    } catch (error) {
      console.error("microphone", error);
      release();
      if (current()) this.#state[1]("off");
      return;
    }
    if (!current()) {
      release();
      return;
    }
    const node = new AudioWorkletNode(context, "farglass-mic", { numberOfInputs: 1, numberOfOutputs: 0, channelCount: 1, channelCountMode: "explicit" });
    context.createMediaStreamSource(stream).connect(node);
    const encoder = new AudioEncoder({ output: (chunk) => this.#send(chunk), error: (error) => console.error("microphone", error) });
    encoder.configure({
      codec: format.codec,
      sampleRate: format.sample_rate,
      numberOfChannels: format.channels,
      bitrate: BITRATE,
      opus: { frameDuration: 20_000, usedtx: true },
    });
    node.port.onmessage = (message: MessageEvent<Float32Array<ArrayBuffer>>) => this.#encode(message.data);
    this.#stream = stream;
    this.#context = context;
    this.#encoder = encoder;
    this.#seq = 0;
    this.#frames = 0;
    this.#channel.control({ type: "start" } satisfies MicRequest);
    this.#state[1]("on");
    // An unplugged device, or a block in the site settings, ends the track.
    for (const track of stream.getTracks()) {
      track.addEventListener("ended", () => {
        if (this.#stream === stream) this.stop();
      });
    }
  }

  /** Turns the microphone off and gives the device back to the browser. */
  stop(): void {
    if (this.#state[0]() === "on") this.#channel.control({ type: "stop" } satisfies MicRequest);
    this.#off();
  }

  close(): void {
    this.#off();
    this.#channel.close();
  }

  #event(event: MicEvent): void {
    switch (event.type) {
      case "ready":
        this.#format = event;
        this.#available[1](micSupported());
        break;
      case "unavailable":
        this.#format = null;
        this.#available[1](false);
        break;
      case "taken":
        // A different tab or computer of the user turned the microphone on.
        this.#off();
        break;
    }
  }

  #off(): void {
    this.#generation++;
    for (const track of this.#stream?.getTracks() ?? []) track.stop();
    if (this.#encoder && this.#encoder.state !== "closed") this.#encoder.close();
    void this.#context?.close().catch(() => {});
    this.#stream = null;
    this.#encoder = null;
    this.#context = null;
    this.#state[1]("off");
  }

  #encode(samples: Float32Array<ArrayBuffer>): void {
    const encoder = this.#encoder;
    const format = this.#format;
    if (!encoder || encoder.state !== "configured" || !format) return;
    const data = new AudioData({
      format: "f32-planar",
      sampleRate: format.sample_rate,
      numberOfFrames: samples.length,
      numberOfChannels: 1,
      timestamp: Math.round((this.#frames * 1_000_000) / format.sample_rate),
      data: samples,
    });
    this.#frames += samples.length;
    encoder.encode(data);
    data.close();
  }

  #send(chunk: EncodedAudioChunk): void {
    const bytes = new Uint8Array(4 + chunk.byteLength);
    new DataView(bytes.buffer).setUint32(0, this.#seq, true);
    this.#seq = (this.#seq + 1) >>> 0;
    chunk.copyTo(bytes.subarray(4));
    this.#channel.send(bytes);
  }
}

// A plain variable: the desktop sets it before its panel renders.
let active: Microphone | undefined;

/** Returns the microphone of the desktop that shows. */
export function microphone(): Microphone | undefined {
  return active;
}

/** Opens the mic channel for `connection`. Returns the function that closes it and stops the capture. */
export function startMicrophone(connection: Connection): () => void {
  const mic = new Microphone(connection);
  active = mic;
  return () => {
    mic.close();
    if (active === mic) active = undefined;
  };
}
