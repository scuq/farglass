/**
 * The AudioWorklet of the microphone (F114). It cuts the samples of the
 * microphone into blocks of 20 ms and posts each block to the page.
 *
 * Messages out: one Float32Array for each block, mono.
 */

// The global scope of an AudioWorklet. The DOM library of TypeScript does not declare it.
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void;

class FarglassMic extends AudioWorkletProcessor {
  readonly #size = Math.round(sampleRate * 0.02);
  #block = new Float32Array(this.#size);
  #fill = 0;

  process(inputs: Float32Array[][]): boolean {
    const input = inputs[0]?.[0];
    let at = 0;
    while (input && at < input.length) {
      const count = Math.min(input.length - at, this.#size - this.#fill);
      this.#block.set(input.subarray(at, at + count), this.#fill);
      this.#fill += count;
      at += count;
      if (this.#fill === this.#size) {
        this.port.postMessage(this.#block, [this.#block.buffer]);
        this.#block = new Float32Array(this.#size);
        this.#fill = 0;
      }
    }
    return true;
  }
}

registerProcessor("farglass-mic", FarglassMic);
