/**
 * The worker of one surface: it decodes the tile stream and draws on an
 * OffscreenCanvas, thus the decode does not block the shell.
 *
 * Messages in: `{ canvas }` once, then `{ bytes }` for each data frame.
 * Messages out: `{ type: "size", width, height }`, `{ type: "end", seq }`, and
 * `{ type: "video", frames }` with the count of decoded video frames, and
 * `{ type: "video-failed" }` if this browser cannot decode the video (F037).
 * `{ key }` sets the rectangle where black is transparent, or none (F040).
 */

import { avcCodec, SurfaceDecoder, VideoFlag } from "./decoder";

let context: OffscreenCanvasRenderingContext2D | null = null;
let canvas: OffscreenCanvas | null = null;
const waiting: Uint8Array[] = [];
// The data frames decode one after the other: a lossy tile waits for its decode.
let queue: Promise<void> = Promise.resolve();
/** The time with no picture after which the decoder counts as broken, while frames go to it. */
const VIDEO_STALL_MS = 2000;

let videoFrames = 0;
/** Reports whether the worker told the shell that it cannot decode the video. */
let videoFailed = false;

/**
 * The H.264 decoder of the video region (F037). A decoder can hold a frame until the next one
 * comes, thus the stream does not wait for a picture: each picture draws at the place of its frame.
 */
let video: {
  decoder: VideoDecoder;
  places: Map<number, { x: number; y: number; w: number; h: number }>;
  time: number;
  lastOutput: number;
} | null = null;

/** Tells the shell that this browser cannot decode the video: the agent then sends tiles only. */
function failVideo(reason: unknown): void {
  console.warn("surface video: the browser cannot decode it, the agent sends tiles", reason);
  endVideo();
  if (!videoFailed) {
    videoFailed = true;
    postMessage({ type: "video-failed" });
  }
}

function endVideo(): void {
  if (!video) return;
  const old = video;
  video = null;
  // A close drops the pictures that did not come yet: the exact tiles after the end stay.
  if (old.decoder.state !== "closed") old.decoder.close();
}

function startVideo(codec: string): void {
  endVideo();
  const current: NonNullable<typeof video> = {
    places: new Map(),
    time: 0,
    lastOutput: performance.now(),
    decoder: new VideoDecoder({
      output(frame) {
        const place = current.places.get(frame.timestamp);
        current.places.delete(frame.timestamp);
        current.lastOutput = performance.now();
        if (place && video === current) {
          context?.drawImage(frame, place.x, place.y, place.w, place.h);
          videoFrames += 1;
          postMessage({ type: "video", frames: videoFrames });
        }
        frame.close();
      },
      error(error) {
        if (video === current) failVideo(error);
      },
    }),
  };
  const config = { codec, optimizeForLatency: true };
  video = current;
  void VideoDecoder.isConfigSupported(config).then(
    (support) => {
      if (!support.supported && video === current) failVideo(`${codec} is not supported`);
    },
    (error: unknown) => video === current && failVideo(error),
  );
  try {
    current.decoder.configure(config);
  } catch (error) {
    failVideo(error);
  }
}

/**
 * The rectangle of a redirected video, in buffer pixels (F040). The host draws the video
 * black, and the black pixels in this rectangle are transparent: the video element of the
 * client under the canvas shows through, and the controls of the page stay on top.
 */
type KeyRect = { x: number; y: number; width: number; height: number };
let keyRect: KeyRect | null = null;

/** Returns the part of the rectangle (x, y, w, h) in the key rectangle, or none. */
function keyPart(x: number, y: number, w: number, h: number): KeyRect | null {
  if (!keyRect) return null;
  const x0 = Math.max(x, keyRect.x);
  const y0 = Math.max(y, keyRect.y);
  const x1 = Math.min(x + w, keyRect.x + keyRect.width);
  const y1 = Math.min(y + h, keyRect.y + keyRect.height);
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, width: x1 - x0, height: y1 - y0 } : null;
}

/** Makes the black pixels of `part` of the canvas transparent, or opaque again. */
function rekey(part: KeyRect, transparent: boolean): void {
  if (!context) return;
  const image = context.getImageData(part.x, part.y, part.width, part.height);
  const data = image.data;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i] === 0 && data[i + 1] === 0 && data[i + 2] === 0) data[i + 3] = transparent ? 0 : 255;
  }
  context.putImageData(image, part.x, part.y);
}

function setKey(next: KeyRect | null): void {
  const same = next && keyRect && next.x === keyRect.x && next.y === keyRect.y && next.width === keyRect.width && next.height === keyRect.height;
  if (same || (!next && !keyRect)) return;
  if (keyRect) rekey(keyRect, false);
  keyRect = next;
  if (canvas && keyRect) {
    const part = keyPart(0, 0, canvas.width, canvas.height);
    if (part) rekey(part, true);
  }
}

const push = (bytes: Uint8Array) => {
  queue = queue.then(() => decoder.push(bytes)).catch((error: unknown) => console.error("surface stream", error));
};

const decoder = new SurfaceDecoder({
  resize(width, height) {
    if (canvas) {
      canvas.width = width;
      canvas.height = height;
    }
    postMessage({ type: "size", width, height });
  },
  put(x, y, width, height, rgba) {
    const part = keyPart(x, y, width, height);
    if (part) {
      // The cache keeps `rgba`: the transparent black goes into a copy.
      const keyed = rgba.slice();
      for (let row: number = part.y - y; row < part.y - y + part.height; row++) {
        for (let column: number = part.x - x; column < part.x - x + part.width; column++) {
          const i = (row * width + column) * 4;
          if (keyed[i] === 0 && keyed[i + 1] === 0 && keyed[i + 2] === 0) keyed[i + 3] = 0;
        }
      }
      context?.putImageData(new ImageData(keyed, width, height), x, y);
      return;
    }
    context?.putImageData(new ImageData(rgba, width, height), x, y);
  },
  move(x, y, width, height, dy) {
    if (context) {
      context.putImageData(context.getImageData(x, y, width, height), x, y + dy);
    }
  },
  async jpeg(x, y, width, height, jpeg) {
    const bitmap = await createImageBitmap(new Blob([jpeg], { type: "image/jpeg" }));
    context?.clearRect(x, y, width, height);
    context?.drawImage(bitmap, x, y, width, height);
    bitmap.close();
  },
  video(x, y, w, h, flags, data) {
    if (videoFailed) return Promise.resolve();
    if (flags & VideoFlag.NEW || !video) {
      // After an error, a frame that is not a key frame cannot decode: it waits for the end of the video.
      if (!(flags & VideoFlag.KEY)) return Promise.resolve();
      startVideo(avcCodec(data) ?? "avc1.42c01f");
    }
    const current = video;
    if (!current || current.decoder.state !== "configured") return Promise.resolve();
    // Frames go to the decoder, but no picture comes: the decoder is broken.
    if (current.places.size > 0 && performance.now() - current.lastOutput > VIDEO_STALL_MS) {
      failVideo("no picture for 2 s");
      return Promise.resolve();
    }
    current.places.set(current.time, { x, y, w, h });
    try {
      current.decoder.decode(new EncodedVideoChunk({ type: flags & VideoFlag.KEY ? "key" : "delta", timestamp: current.time, data }));
    } catch (error) {
      failVideo(error);
    }
    current.time += 33_333;
    return Promise.resolve();
  },
  videoEnd() {
    endVideo();
  },
  end(seq) {
    postMessage({ type: "end", seq });
  },
});

onmessage = (event: MessageEvent<{ canvas?: OffscreenCanvas; bytes?: ArrayBuffer; key?: KeyRect | null }>) => {
  if ("key" in event.data) {
    // The key changes after the frames that came before it.
    const next = event.data.key ?? null;
    queue = queue.then(() => setKey(next));
    return;
  }
  if (event.data.canvas) {
    canvas = event.data.canvas;
    context = canvas.getContext("2d", { alpha: true });
    for (const bytes of waiting.splice(0)) {
      push(bytes);
    }
    return;
  }
  if (event.data.bytes) {
    const bytes = new Uint8Array(event.data.bytes);
    if (context) {
      push(bytes);
    } else {
      waiting.push(bytes);
    }
  }
};
