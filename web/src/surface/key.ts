/**
 * The color key of a redirected video (F040). The host hides the video and draws black under the
 * controls of the page. In the rectangle of the video, the canvas of the window makes that black
 * transparent, thus the video element of the client under the canvas shows.
 *
 * A semi-transparent element of the page over the video comes from the host as gray over black.
 * A very dark gray is a dark panel, for example a menu or the background of the captions: it
 * becomes a dark gray with the alpha {@link DARK_ALPHA}. A brighter gray is a white overlay, for
 * example the graph of the most replayed parts: white with an alpha `a` over black is the gray
 * `255 a`, thus it becomes white with that alpha. Pure black becomes transparent, and a colored or
 * bright pixel stays opaque. The change is reversible: {@link unkeyPixel} gives the gray back.
 */

/** The largest difference of the channels of a gray pixel. */
export const GRAY_SPREAD = 12;

/** A gray from this value up stays opaque: an image or an element, not an overlay. White up to about 39 % over black is below it. */
export const GRAY_MAX = 100;

/** A gray up to this value is a dark panel over the video, not a white overlay. */
export const DARK_MAX = 48;

/** The alpha of a dark panel, about the alpha of the menus and the captions of video players. */
export const DARK_ALPHA = 217;

/**
 * Keys the pixel at `i` of RGBA `data` that is not premultiplied. `tolerance` is the largest channel
 * value of black. With `gray`, a dark gray becomes white with an alpha: only for exact pixels, because
 * a lossy picture, for example the preview of the seek bar, has dark grays of its own.
 */
export function keyPixel(data: Uint8ClampedArray, i: number, tolerance: number, gray = true): void {
  const r = data[i]!;
  const g = data[i + 1]!;
  const b = data[i + 2]!;
  if (data[i + 3] !== 255) return;
  const max = Math.max(r, g, b);
  if (max <= tolerance) {
    data[i + 3] = 0;
    return;
  }
  if (gray && max - Math.min(r, g, b) <= GRAY_SPREAD && max < GRAY_MAX) {
    if (max <= DARK_MAX) {
      // A dark panel: its color is the gray over black divided by its alpha.
      const value = Math.min(254, Math.round((max * 255) / DARK_ALPHA));
      data[i] = value;
      data[i + 1] = value;
      data[i + 2] = value;
      data[i + 3] = DARK_ALPHA;
      return;
    }
    data[i] = 255;
    data[i + 1] = 255;
    data[i + 2] = 255;
    data[i + 3] = max;
  }
}

/** Gives the pixel at `i` back its opaque value: black for a transparent pixel, the gray for a keyed gray. */
export function unkeyPixel(data: Uint8ClampedArray, i: number): void {
  const a = data[i + 3]!;
  if (a === 0) {
    data[i] = 0;
    data[i + 1] = 0;
    data[i + 2] = 0;
    data[i + 3] = 255;
  } else if (a < 255 && data[i] === 255 && data[i + 1] === 255 && data[i + 2] === 255) {
    data[i] = a;
    data[i + 1] = a;
    data[i + 2] = a;
    data[i + 3] = 255;
  } else if (a === DARK_ALPHA && data[i] === data[i + 1] && data[i] === data[i + 2]) {
    const value = Math.round((data[i]! * DARK_ALPHA) / 255);
    data[i] = value;
    data[i + 1] = value;
    data[i + 2] = value;
    data[i + 3] = 255;
  }
}

/** The size of the blocks of {@link keyArea}: the tiles of the stream. */
export const KEY_BLOCK = 64;

/** The largest part of colored pixels in a block of an overlay: a picture has more, controls have a few, as a red bar. */
export const COLORED_SHARE = 0.2;

/** Reports whether the block is an overlay: gray and black, with few colored pixels. A picture has many. */
export function isGrayBlock(data: Uint8ClampedArray, width: number, x0: number, y0: number, w: number, h: number): boolean {
  const limit = w * h * COLORED_SHARE;
  let colored = 0;
  for (let y = y0; y < y0 + h; y++) {
    for (let x = x0; x < x0 + w; x++) {
      const i = (y * width + x) * 4;
      const r = data[i]!;
      const g = data[i + 1]!;
      const b = data[i + 2]!;
      if (Math.max(r, g, b) - Math.min(r, g, b) > GRAY_SPREAD && ++colored > limit) return false;
    }
  }
  return true;
}

/**
 * Keys the area (x0, y0, w, h) of RGBA `data` with the row length `width`, in blocks of
 * {@link KEY_BLOCK} from (`gridX`, `gridY`). The gray rule applies only in a block with no colors:
 * the dark grays of a picture, for example the preview of the seek bar, stay opaque.
 */
export function keyArea(data: Uint8ClampedArray, width: number, x0: number, y0: number, w: number, h: number, tolerance: number, gridX = 0, gridY = 0): void {
  const firstX = x0 - ((((x0 - gridX) % KEY_BLOCK) + KEY_BLOCK) % KEY_BLOCK);
  const firstY = y0 - ((((y0 - gridY) % KEY_BLOCK) + KEY_BLOCK) % KEY_BLOCK);
  for (let by = firstY; by < y0 + h; by += KEY_BLOCK) {
    for (let bx = firstX; bx < x0 + w; bx += KEY_BLOCK) {
      const left = Math.max(bx, x0);
      const top = Math.max(by, y0);
      const right = Math.min(bx + KEY_BLOCK, x0 + w);
      const bottom = Math.min(by + KEY_BLOCK, y0 + h);
      const gray = isGrayBlock(data, width, left, top, right - left, bottom - top);
      for (let y = top; y < bottom; y++) {
        for (let x = left; x < right; x++) keyPixel(data, (y * width + x) * 4, tolerance, gray);
      }
    }
  }
}
