import { describe, expect, test } from "vitest";
import { JitterBuffer } from "./jitter";

const ramp = (from: number, count: number) => Float32Array.from({ length: count }, (_, i) => from + i);
const pull = (buffer: JitterBuffer, count = 4) => {
  const out = [new Float32Array(count), new Float32Array(count)];
  buffer.pull(out);
  return [...out[0]!];
};

describe("JitterBuffer", () => {
  test("it waits for the target, plays in order across the end of the ring, and waits again after it ran empty", () => {
    const buffer = new JitterBuffer(2, 10, 6, 9);
    buffer.push([ramp(1, 4), ramp(1, 4)]);
    expect(pull(buffer)).toEqual([0, 0, 0, 0]);
    buffer.push([ramp(5, 4), ramp(5, 4)]);
    expect(pull(buffer)).toEqual([1, 2, 3, 4]);
    buffer.push([ramp(9, 4), ramp(9, 4)]);
    expect(pull(buffer)).toEqual([5, 6, 7, 8]);
    expect(pull(buffer)).toEqual([9, 10, 11, 12]);
    expect(buffer.playing).toBe(true);
    expect(pull(buffer)).toEqual([0, 0, 0, 0]);
    expect(buffer.playing).toBe(false);
    expect(buffer.underruns).toBe(1);
  });

  test("a part of a pull plays and the rest is silence", () => {
    const buffer = new JitterBuffer(1, 16, 2, 12);
    buffer.push([ramp(1, 3)]);
    const out = [new Float32Array(4)];
    buffer.pull(out);
    expect([...out[0]!]).toEqual([1, 2, 3, 0]);
  });

  test("a burst after a stall drops the oldest samples down to the target", () => {
    const buffer = new JitterBuffer(1, 32, 4, 8);
    buffer.push([ramp(1, 10)]);
    expect(buffer.buffered).toBe(4);
    expect(buffer.dropped).toBe(6);
    expect(pull(buffer)).toEqual([7, 8, 9, 10]);
  });
});
