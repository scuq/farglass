import { describe, expect, test } from "vitest";
import { bgraToRgba, Code, SurfaceDecoder } from "./decoder";
import { decodeLz4Block } from "./lz4";

/** A canvas in memory that records what the decoder draws. */
class Canvas {
  width = 0;
  height = 0;
  rgba = new Uint8ClampedArray(0);
  ended: number[] = [];
  readonly target = {
    resize: (w: number, h: number) => {
      this.width = w;
      this.height = h;
      this.rgba = new Uint8ClampedArray(w * h * 4);
    },
    put: (x: number, y: number, w: number, h: number, rgba: Uint8ClampedArray) => {
      for (let row = 0; row < h; row++) {
        this.rgba.set(rgba.subarray(row * w * 4, (row + 1) * w * 4), ((y + row) * this.width + x) * 4);
      }
    },
    move: (x: number, y: number, w: number, h: number, dy: number) => {
      const block = [];
      for (let row = 0; row < h; row++) block.push(this.rgba.slice(((y + row) * this.width + x) * 4, ((y + row) * this.width + x + w) * 4));
      block.forEach((line, row) => this.rgba.set(line, ((y + row + dy) * this.width + x) * 4));
    },
    end: (seq: number) => this.ended.push(seq),
  };
  pixel(x: number, y: number) {
    return [...this.rgba.subarray((y * this.width + x) * 4, (y * this.width + x) * 4 + 4)];
  }
}

/** Writes the messages of farglass-surface. */
class Stream {
  bytes: number[] = [];
  u8(v: number) {
    this.bytes.push(v & 255);
    return this;
  }
  u16(v: number) {
    return this.u8(v).u8(v >> 8);
  }
  u32(v: number) {
    return this.u16(v & 0xffff).u16(v >>> 16);
  }
  get array() {
    return new Uint8Array(this.bytes);
  }
}

/** An LZ4 block with only literals: valid input for each decoder. */
function literalBlock(data: number[]): number[] {
  const n = data.length;
  const head = n >= 15 ? [0xf0, ...Array(Math.floor((n - 15) / 255)).fill(255), (n - 15) % 255] : [n << 4];
  return [...head, ...data];
}

describe("surface decoder", () => {
  test("LZ4 decodes literals and an overlapping match", () => {
    // "ab" then a match of offset 2 and length 6: "abababab".
    const block = new Uint8Array([0x22, 97, 98, 2, 0, 0x00]);
    expect(new TextDecoder().decode(decodeLz4Block(block.subarray(0, 5), 8))).toBe("abababab");
    expect([...decodeLz4Block(new Uint8Array(literalBlock([1, 2, 3])), 3)]).toEqual([1, 2, 3]);
    expect(() => decodeLz4Block(new Uint8Array([0x10, 1, 9, 0]), 8)).toThrow();
  });

  test("premultiplied BGRA becomes straight RGBA", () => {
    expect([...bgraToRgba(new Uint8Array([10, 20, 30, 255, 0, 0, 64, 128, 0, 0, 0, 0]))]).toEqual([30, 20, 10, 255, 128, 0, 0, 128, 0, 0, 0, 0]);
  });

  test("a frame with a tile and a fill, split over data frames, draws and ends", () => {
    const canvas = new Canvas();
    const decoder = new SurfaceDecoder(canvas.target);
    const tile = literalBlock([1, 2, 3, 255, 4, 5, 6, 255]);
    const s = new Stream().u8(Code.HELLO).u8(1).u8(64).u16(2048);
    s.u8(Code.FRAME).u32(1).u16(4).u16(2);
    s.u8(Code.TILE).u16(0).u16(0).u8(2).u8(1).u32(tile.length);
    s.bytes.push(...tile);
    s.u8(Code.FILL).u16(0).u16(1).u8(4).u8(1).u32(0xff332211);
    s.u8(Code.END).u32(1);
    const all = s.array;
    for (let i = 0; i < all.length; i += 5) {
      decoder.push(all.subarray(i, i + 5));
    }
    expect(decoder.slots).toBe(2048);
    expect([canvas.width, canvas.height]).toEqual([4, 2]);
    expect(canvas.pixel(0, 0)).toEqual([3, 2, 1, 255]);
    expect(canvas.pixel(1, 0)).toEqual([6, 5, 4, 255]);
    expect(canvas.pixel(3, 1)).toEqual([0x33, 0x22, 0x11, 255]);
    expect(canvas.pixel(3, 0)).toEqual([0, 0, 0, 0]);
    expect(canvas.ended).toEqual([1]);
  });

  test("a tile takes the next cache slot, a reference draws it, and a keep copies it to the next slot", () => {
    const canvas = new Canvas();
    const decoder = new SurfaceDecoder(canvas.target);
    const red = literalBlock([0, 0, 255, 255]);
    const green = literalBlock([0, 255, 0, 255]);
    const s = new Stream().u8(Code.HELLO).u8(1).u8(64).u16(2);
    s.u8(Code.FRAME).u32(1).u16(3).u16(1);
    s.u8(Code.TILE).u16(0).u16(0).u8(1).u8(1).u32(red.length);
    s.bytes.push(...red);
    s.u8(Code.TILE).u16(1).u16(0).u8(1).u8(1).u32(green.length);
    s.bytes.push(...green);
    // The ring has 2 slots: red in 0, green in 1. A keep of slot 0 copies red to slot 0 again (the next slot).
    s.u8(Code.KEEP).u16(0);
    s.u8(Code.REF).u16(2).u16(0).u16(0);
    s.u8(Code.END).u32(1);
    decoder.push(s.array);
    expect([canvas.pixel(0, 0), canvas.pixel(1, 0), canvas.pixel(2, 0)]).toEqual([
      [255, 0, 0, 255],
      [0, 255, 0, 255],
      [255, 0, 0, 255],
    ]);
    expect(() => decoder.push(new Stream().u8(Code.REF).u16(0).u16(0).u16(1).array)).not.toThrow();
  });

  test("a move copies a block of the canvas up, as a scroll does", () => {
    const canvas = new Canvas();
    const decoder = new SurfaceDecoder(canvas.target);
    const s = new Stream().u8(Code.FRAME).u32(1).u16(1).u16(3);
    s.u8(Code.FILL).u16(0).u16(1).u8(1).u8(1).u32(0xff0000ff);
    s.u8(Code.FILL).u16(0).u16(2).u8(1).u8(1).u32(0xff00ff00);
    // Rows 1 and 2 go one row up.
    s.u8(Code.MOVE).u16(0).u16(1).u16(1).u16(2).u16(0xffff);
    s.u8(Code.END).u32(1);
    decoder.push(s.array);
    // The fills are BGRA: 0xff0000ff is blue.
    expect([canvas.pixel(0, 0), canvas.pixel(0, 1)]).toEqual([
      [0, 0, 255, 255],
      [0, 255, 0, 255],
    ]);
  });

  test("an unknown message stops the stream", () => {
    const decoder = new SurfaceDecoder(new Canvas().target);
    expect(() => decoder.push(new Uint8Array([99]))).toThrow("unknown message 99");
  });
});
