/**
 * The jitter buffer of the sound (F038): a ring of samples for each channel.
 *
 * The packets come with a jitter, but the audio device takes 128 samples at
 * a fixed rate. The buffer plays only after it has `target` samples. If it
 * runs empty, it plays silence and waits for `target` samples again. If it
 * holds more than `max` samples, for example after a stall of the
 * connection, it drops the oldest samples down to `target`: the delay of the
 * sound stays short.
 */
export class JitterBuffer {
  readonly #rings: Float32Array[];
  readonly #capacity: number;
  #read = 0;
  #size = 0;
  /** Reports whether the buffer plays: false while it fills. */
  playing = false;
  /** The times that the buffer ran empty while it played. */
  underruns = 0;
  /** The samples that the buffer dropped to keep the delay short. */
  dropped = 0;

  constructor(
    channels: number,
    capacity: number,
    readonly target: number,
    readonly max: number,
  ) {
    this.#capacity = capacity;
    this.#rings = Array.from({ length: channels }, () => new Float32Array(capacity));
  }

  /** The samples of each channel in the buffer. */
  get buffered(): number {
    return this.#size;
  }

  #drop(count: number): void {
    this.#read = (this.#read + count) % this.#capacity;
    this.#size -= count;
    this.dropped += count;
  }

  /** Adds samples: one array for each channel, of the same length. */
  push(planes: readonly Float32Array[]): void {
    const count = Math.min(planes[0]?.length ?? 0, this.#capacity);
    if (this.#size + count > this.#capacity) {
      this.#drop(this.#size + count - this.#capacity);
    }
    const write = (this.#read + this.#size) % this.#capacity;
    this.#rings.forEach((ring, channel) => {
      const plane = planes[Math.min(channel, planes.length - 1)]!;
      const first = Math.min(count, this.#capacity - write);
      ring.set(plane.subarray(plane.length - count, plane.length - count + first), write);
      ring.set(plane.subarray(plane.length - count + first, plane.length), 0);
    });
    this.#size += count;
    if (this.#size > this.max) {
      this.#drop(this.#size - this.target);
    }
  }

  /** Fills `out`, one array for each channel, with the next samples, or with silence. */
  pull(out: readonly Float32Array[]): void {
    const count = out[0]?.length ?? 0;
    if (!this.playing && this.#size >= this.target) {
      this.playing = true;
    }
    const take = this.playing ? Math.min(count, this.#size) : 0;
    out.forEach((plane, channel) => {
      const ring = this.#rings[Math.min(channel, this.#rings.length - 1)]!;
      const first = Math.min(take, this.#capacity - this.#read);
      plane.set(ring.subarray(this.#read, this.#read + first), 0);
      plane.set(ring.subarray(0, take - first), first);
      plane.fill(0, take);
    });
    this.#read = (this.#read + take) % this.#capacity;
    this.#size -= take;
    if (this.playing && take < count) {
      this.playing = false;
      this.underruns += 1;
    }
  }
}
