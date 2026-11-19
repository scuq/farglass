/**
 * The decoder of the tile stream of a surface (F025). It reads the messages of
 * farglass-surface and gives the pixels to a target. The data frames can split
 * a message: the decoder keeps the rest until the next data frame.
 */

import { decodeLz4Block } from "./lz4";

/** The message codes. They are the same as in crates/farglass-surface/src/tiles.rs. */
export const Code = { HELLO: 0, FRAME: 1, TILE: 2, REF: 3, KEEP: 5, FILL: 6, END: 8 } as const;

/** One cache slot: the pixels of a tile, ready for the target. */
interface Slot {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8ClampedArray<ArrayBuffer>;
}

/** Where the decoder draws. */
export interface SurfaceTarget {
  /** A new size: the canvas is transparent black. */
  resize(width: number, height: number): void;
  /** Replaces the pixels of a rectangle with `rgba`, not premultiplied. */
  put(x: number, y: number, width: number, height: number, rgba: Uint8ClampedArray<ArrayBuffer>): void;
  /** The frame is complete. */
  end(seq: number): void;
}

/** Converts premultiplied BGRA from Wayland to RGBA that is not premultiplied, for ImageData. */
export function bgraToRgba(bgra: Uint8Array): Uint8ClampedArray<ArrayBuffer> {
  const rgba = new Uint8ClampedArray(bgra.length);
  for (let i = 0; i < bgra.length; i += 4) {
    const a = bgra[i + 3]!;
    if (a === 255) {
      rgba[i] = bgra[i + 2]!;
      rgba[i + 1] = bgra[i + 1]!;
      rgba[i + 2] = bgra[i]!;
    } else if (a > 0) {
      rgba[i] = (bgra[i + 2]! * 255) / a;
      rgba[i + 1] = (bgra[i + 1]! * 255) / a;
      rgba[i + 2] = (bgra[i]! * 255) / a;
    }
    rgba[i + 3] = a;
  }
  return rgba;
}

/** Sizes of the fixed fields after the code byte. */
const FIXED: Record<number, number> = {
  [Code.HELLO]: 4,
  [Code.FRAME]: 8,
  [Code.TILE]: 10,
  [Code.REF]: 6,
  [Code.KEEP]: 2,
  [Code.FILL]: 10,
  [Code.END]: 4,
};

export class SurfaceDecoder {
  readonly #target: SurfaceTarget;
  #pending = new Uint8Array(0);
  width = 0;
  height = 0;
  /** The cache slots that the host gave in its hello message (ADR-011). Both sides take the slots in the same order. */
  slots = 0;
  #ring: (Slot | undefined)[] = [];
  #next = 0;

  #store(slot: Slot | undefined): void {
    if (this.#ring.length === 0) {
      return;
    }
    this.#ring[this.#next] = slot;
    this.#next = (this.#next + 1) % this.#ring.length;
  }

  constructor(target: SurfaceTarget) {
    this.#target = target;
  }

  /** Takes the payload of one data frame. */
  push(chunk: Uint8Array): void {
    let bytes: Uint8Array;
    if (this.#pending.length === 0) {
      bytes = chunk;
    } else {
      bytes = new Uint8Array(this.#pending.length + chunk.length);
      bytes.set(this.#pending);
      bytes.set(chunk, this.#pending.length);
    }
    let offset = 0;
    for (;;) {
      const used = this.#message(bytes, offset);
      if (used === 0) {
        break;
      }
      offset += used;
    }
    this.#pending = bytes.slice(offset);
  }

  /** Handles the message at `offset`. Returns its length, or 0 if it is not complete. */
  #message(bytes: Uint8Array, offset: number): number {
    if (offset >= bytes.length) {
      return 0;
    }
    const code = bytes[offset]!;
    const fixed = FIXED[code];
    if (fixed === undefined) {
      throw new Error(`The surface stream has the unknown message ${code}.`);
    }
    if (bytes.length - offset < 1 + fixed) {
      return 0;
    }
    const view = new DataView(bytes.buffer, bytes.byteOffset + offset + 1, fixed);
    switch (code) {
      case Code.HELLO:
        this.slots = view.getUint16(2, true);
        this.#ring = new Array(this.slots);
        this.#next = 0;
        return 1 + fixed;
      case Code.FRAME: {
        const width = view.getUint16(4, true);
        const height = view.getUint16(6, true);
        if (width !== this.width || height !== this.height) {
          this.width = width;
          this.height = height;
          this.#target.resize(width, height);
        }
        return 1 + fixed;
      }
      case Code.TILE: {
        const length = view.getUint32(6, true);
        const end = offset + 1 + fixed + length;
        if (bytes.length < end) {
          return 0;
        }
        const [x, y, w, h] = [view.getUint16(0, true), view.getUint16(2, true), view.getUint8(4), view.getUint8(5)];
        const rgba = bgraToRgba(decodeLz4Block(bytes.subarray(offset + 1 + fixed, end), w * h * 4));
        this.#store({ width: w, height: h, rgba });
        this.#target.put(x, y, w, h, rgba);
        return end - offset;
      }
      case Code.REF: {
        const slot = this.#ring[view.getUint16(4, true)];
        if (!slot) {
          throw new Error("The surface stream names an empty cache slot.");
        }
        this.#target.put(view.getUint16(0, true), view.getUint16(2, true), slot.width, slot.height, slot.rgba);
        return 1 + fixed;
      }
      case Code.KEEP:
        this.#store(this.#ring[view.getUint16(0, true)]);
        return 1 + fixed;
      case Code.FILL: {
        const [x, y, w, h] = [view.getUint16(0, true), view.getUint16(2, true), view.getUint8(4), view.getUint8(5)];
        const pixel = bgraToRgba(bytes.subarray(offset + 7, offset + 11));
        const rgba = new Uint8ClampedArray(w * h * 4);
        for (let i = 0; i < rgba.length; i += 4) {
          rgba.set(pixel, i);
        }
        this.#target.put(x, y, w, h, rgba);
        return 1 + fixed;
      }
      default:
        this.#target.end(view.getUint32(0, true));
        return 1 + fixed;
    }
  }
}
