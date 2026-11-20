/**
 * The sound of the apps of the session (F038).
 *
 * The player keeps an audio channel open while the desktop shows. Each data
 * frame holds one Opus packet. WebCodecs decodes it, and an AudioWorklet
 * plays the samples through a jitter buffer. The browser starts the sound
 * only after an action of the user, thus the first press or key resumes it.
 */

import { createSignal } from "solid-js";
import type { AudioEvent, Channel, Connection } from "../proto";
import workletUrl from "./worklet.ts?worker&url";

/** The state of the worklet. */
export interface SoundState {
  readonly buffered: number;
  readonly playing: boolean;
  readonly underruns: number;
  readonly level: number;
}

export class SoundPlayer {
  readonly #channel: Channel;
  #context: AudioContext | null = null;
  #node: AudioWorkletNode | null = null;
  #gain: GainNode | null = null;
  #decoder: AudioDecoder | null = null;
  readonly #available = createSignal(false);
  readonly #muted = createSignal(false);
  readonly #packets = createSignal(0);
  readonly #state = createSignal<SoundState | null>(null);
  readonly #resume = () => void this.#context?.resume().catch(() => {});

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "audio" },
      { oncontrol: (message) => void this.#event(message as AudioEvent), ondata: (data) => this.#packet(data) },
    );
    window.addEventListener("pointerdown", this.#resume, true);
    window.addEventListener("keydown", this.#resume, true);
  }

  /** Reports whether the session has sound. */
  available(): boolean {
    return this.#available[0]();
  }

  /** Reports whether the sound is off. */
  muted(): boolean {
    return this.#muted[0]();
  }

  /** The packets that came. */
  packets(): number {
    return this.#packets[0]();
  }

  /** The last state of the worklet, or null before the sound starts. */
  state(): SoundState | null {
    return this.#state[0]();
  }

  setMuted(muted: boolean): void {
    this.#muted[1](muted);
    if (this.#gain) this.#gain.gain.value = muted ? 0 : 1;
  }

  close(): void {
    window.removeEventListener("pointerdown", this.#resume, true);
    window.removeEventListener("keydown", this.#resume, true);
    this.#channel.close();
    if (this.#decoder && this.#decoder.state !== "closed") this.#decoder.close();
    void this.#context?.close().catch(() => {});
  }

  async #event(event: AudioEvent): Promise<void> {
    if (event.type === "unavailable") {
      this.#available[1](false);
      return;
    }
    if (typeof AudioDecoder === "undefined" || typeof AudioContext === "undefined") {
      console.warn("This browser has no WebCodecs audio decoder: the apps play no sound here.");
      return;
    }
    const context = new AudioContext({ sampleRate: event.sample_rate, latencyHint: "interactive" });
    await context.audioWorklet.addModule(workletUrl);
    const node = new AudioWorkletNode(context, "farglass-sound", { numberOfInputs: 0, outputChannelCount: [event.channels] });
    const gain = context.createGain();
    gain.gain.value = this.#muted[0]() ? 0 : 1;
    node.connect(gain).connect(context.destination);
    node.port.onmessage = (message: MessageEvent<SoundState>) => this.#state[1](message.data);
    const decoder = new AudioDecoder({
      output: (data) => {
        const planes = Array.from({ length: data.numberOfChannels }, (_, plane) => {
          const samples = new Float32Array(data.numberOfFrames);
          data.copyTo(samples, { planeIndex: plane, format: "f32-planar" });
          return samples;
        });
        data.close();
        node.port.postMessage({ planes }, planes.map((plane) => plane.buffer));
      },
      error: (error) => console.error("sound", error),
    });
    decoder.configure({ codec: event.codec, sampleRate: event.sample_rate, numberOfChannels: event.channels });
    this.#context = context;
    this.#node = node;
    this.#gain = gain;
    this.#decoder = decoder;
    this.#available[1](true);
  }

  #packet(data: Uint8Array): void {
    const decoder = this.#decoder;
    if (!decoder || decoder.state !== "configured" || data.length < 5) return;
    const seq = new DataView(data.buffer, data.byteOffset, 4).getUint32(0, true);
    decoder.decode(new EncodedAudioChunk({ type: "key", timestamp: seq * 20_000, data: data.subarray(4) }));
    this.#packets[1](this.#packets[0]() + 1);
  }
}

// A plain variable: the desktop sets it before its panel renders.
let active: SoundPlayer | undefined;

/** Returns the sound of the desktop that shows. */
export function soundPlayer(): SoundPlayer | undefined {
  return active;
}

/** Starts the sound for `connection`. Returns the function that stops it. */
export function startSound(connection: Connection): () => void {
  const player = new SoundPlayer(connection);
  active = player;
  return () => {
    player.close();
    if (active === player) active = undefined;
  };
}
