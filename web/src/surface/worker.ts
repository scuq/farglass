/**
 * The worker of one surface: it decodes the tile stream and draws on an
 * OffscreenCanvas, thus the decode does not block the shell.
 *
 * Messages in: `{ canvas }` once, then `{ bytes }` for each data frame.
 * Messages out: `{ type: "size", width, height }`, `{ type: "end", seq }`, and
 * `{ type: "video", frames }` with the count of decoded video frames (F037).
 */

import { avcCodec, SurfaceDecoder, VideoFlag } from "./decoder";

let context: OffscreenCanvasRenderingContext2D | null = null;
let canvas: OffscreenCanvas | null = null;
const waiting: Uint8Array[] = [];
// The data frames decode one after the other: a lossy tile waits for its decode.
let queue: Promise<void> = Promise.resolve();
/** The longest wait for a video frame: a decoder that gives no picture must not stop the stream. */
const VIDEO_WAIT_MS = 1000;

/** The H.264 decoder of the video region (F037), and the frames that wait for their picture. */
let videoFrames = 0;
let video: { decoder: VideoDecoder; waiting: { x: number; y: number; w: number; h: number; done: () => void }[]; time: number } | null = null;

function endVideo(): void {
  if (!video) return;
  const old = video;
  video = null;
  for (const frame of old.waiting.splice(0)) frame.done();
  if (old.decoder.state !== "closed") old.decoder.close();
}

function startVideo(codec: string): void {
  endVideo();
  const current: NonNullable<typeof video> = {
    waiting: [],
    time: 0,
    decoder: new VideoDecoder({
      output(frame) {
        const place = current.waiting.shift();
        if (place) {
          context?.drawImage(frame, place.x, place.y, place.w, place.h);
          videoFrames += 1;
          postMessage({ type: "video", frames: videoFrames });
          place.done();
        }
        frame.close();
      },
      error(error) {
        console.error("surface video", error);
        if (video === current) endVideo();
      },
    }),
  };
  current.decoder.configure({ codec, optimizeForLatency: true });
  video = current;
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
    if (flags & VideoFlag.NEW || !video) {
      // A stream with no new-stream flag after a decoder error waits for its next key frame.
      if (!(flags & VideoFlag.KEY)) return Promise.resolve();
      startVideo(avcCodec(data) ?? "avc1.42c01f");
    }
    const current = video!;
    return new Promise<void>((resolve) => {
      const place = { x, y, w, h, done: () => (clearTimeout(timer), resolve()) };
      const timer = setTimeout(() => {
        // No picture in time: the frame does not wait longer, and a late picture does not take its place.
        current.waiting.splice(current.waiting.indexOf(place), 1);
        resolve();
      }, VIDEO_WAIT_MS);
      current.waiting.push(place);
      current.decoder.decode(new EncodedVideoChunk({ type: flags & VideoFlag.KEY ? "key" : "delta", timestamp: current.time, data }));
      current.time += 33_333;
    });
  },
  videoEnd() {
    endVideo();
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
