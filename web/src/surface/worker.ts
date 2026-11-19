/**
 * The worker of one surface: it decodes the tile stream and draws on an
 * OffscreenCanvas, thus the decode does not block the shell.
 *
 * Messages in: `{ canvas }` once, then `{ bytes }` for each data frame.
 * Messages out: `{ type: "size", width, height }` and `{ type: "end", seq }`.
 */

import { SurfaceDecoder } from "./decoder";

let context: OffscreenCanvasRenderingContext2D | null = null;
let canvas: OffscreenCanvas | null = null;
const waiting: Uint8Array[] = [];

const decoder = new SurfaceDecoder({
  resize(width, height) {
    if (canvas) {
      canvas.width = width;
      canvas.height = height;
    }
    postMessage({ type: "size", width, height });
  },
  put(x, y, width, height, rgba) {
    context?.putImageData(new ImageData(rgba, width, height), x, y);
  },
  move(x, y, width, height, dy) {
    if (context) {
      context.putImageData(context.getImageData(x, y, width, height), x, y + dy);
    }
  },
  end(seq) {
    postMessage({ type: "end", seq });
  },
});

onmessage = (event: MessageEvent<{ canvas?: OffscreenCanvas; bytes?: ArrayBuffer }>) => {
  if (event.data.canvas) {
    canvas = event.data.canvas;
    context = canvas.getContext("2d", { alpha: true });
    for (const bytes of waiting.splice(0)) {
      decoder.push(bytes);
    }
    return;
  }
  if (event.data.bytes) {
    const bytes = new Uint8Array(event.data.bytes);
    if (context) {
      decoder.push(bytes);
    } else {
      waiting.push(bytes);
    }
  }
};
