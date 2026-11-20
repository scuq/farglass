import { expect, test } from "vitest";
import { keyArea, keyPixel, unkeyPixel } from "./key";

const keyed = (rgba: number[], tolerance = 0) => {
  const data = new Uint8ClampedArray(rgba);
  keyPixel(data, 0, tolerance);
  return [...data];
};

test("black is transparent, a dark gray is white with its alpha, and a color or a bright pixel stays", () => {
  expect(keyed([0, 0, 0, 255])).toEqual([0, 0, 0, 0]);
  expect(keyed([14, 15, 13, 255])).toEqual([255, 255, 255, 15]);
  expect(keyed([14, 15, 13, 255], 20)).toEqual([14, 15, 13, 0]);
  // White at 30 % over black: the graph of the most replayed parts.
  expect(keyed([77, 77, 77, 255])).toEqual([255, 255, 255, 77]);
  expect(keyed([255, 0, 0, 255])).toEqual([255, 0, 0, 255]);
  expect(keyed([255, 255, 255, 255])).toEqual([255, 255, 255, 255]);
  expect(keyed([200, 200, 200, 255])).toEqual([200, 200, 200, 255]);
  // A mid gray of an image, for example in the preview of the seek bar, stays opaque.
  expect(keyed([130, 128, 126, 255])).toEqual([130, 128, 126, 255]);
  // A pixel that the app made transparent stays as it is.
  expect(keyed([0, 0, 0, 128])).toEqual([0, 0, 0, 128]);
});

test("the key goes back to the exact opaque value", () => {
  for (const pixel of [
    [0, 0, 0, 255],
    [77, 77, 77, 255],
    [255, 0, 0, 255],
  ]) {
    const data = new Uint8ClampedArray(pixel);
    keyPixel(data, 0, 0);
    unkeyPixel(data, 0);
    expect([...data]).toEqual(pixel);
  }
});

test("in a block with a color, the grays of the picture stay opaque, and black still goes", () => {
  // Two blocks of 64 x 1: the left one is an overlay, the right one is a picture with green.
  const data = new Uint8ClampedArray(128 * 4);
  for (let x = 0; x < 128; x++) data.set([77, 77, 77, 255], x * 4);
  data.set([0, 0, 0, 255], 10 * 4);
  data.set([0, 0, 0, 255], 70 * 4);
  data.set([40, 160, 60, 255], 100 * 4);
  keyArea(data, 128, 0, 0, 128, 1, 0);
  expect([...data.subarray(0, 4)]).toEqual([255, 255, 255, 77]);
  expect(data[10 * 4 + 3]).toBe(0);
  expect([...data.subarray(64 * 4, 64 * 4 + 4)]).toEqual([77, 77, 77, 255]);
  expect(data[70 * 4 + 3]).toBe(0);
});
