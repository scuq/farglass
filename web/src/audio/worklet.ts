/**
 * The AudioWorklet of the sound of the apps (F038). It plays the samples of
 * the decoder through a jitter buffer, and reports its state every 250 ms.
 *
 * Messages in: `{ planes }`, one Float32Array for each channel.
 * Messages out: `{ buffered, playing, underruns, level }`.
 */

import { JitterBuffer } from "./jitter";

// The global scope of an AudioWorklet. The DOM library of TypeScript does not declare it.
declare const sampleRate: number;
declare const currentFrame: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void;

class FarglassSound extends AudioWorkletProcessor {
  // 60 ms before the sound starts, at most 200 ms of delay, and room for 2 s.
  readonly #buffer = new JitterBuffer(2, sampleRate * 2, Math.round(sampleRate * 0.06), Math.round(sampleRate * 0.2));
  #next = 0;
  #peak = 0;

  constructor() {
    super();
    this.port.onmessage = (event: MessageEvent<{ planes: Float32Array[] }>) => this.#buffer.push(event.data.planes);
  }

  process(_inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    const out = outputs[0];
    if (out) {
      this.#buffer.pull(out);
      for (const sample of out[0] ?? []) this.#peak = Math.max(this.#peak, Math.abs(sample));
    }
    if (currentFrame >= this.#next) {
      this.#next = currentFrame + sampleRate / 4;
      this.port.postMessage({ buffered: this.#buffer.buffered, playing: this.#buffer.playing, underruns: this.#buffer.underruns, level: this.#peak });
      this.#peak = 0;
    }
    return true;
  }
}

registerProcessor("farglass-sound", FarglassSound);
