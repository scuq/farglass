/**
 * The Worker of the camera (F118, ADR-058): it reads the pictures of the camera, and encodes them off the main thread.
 *
 * The main thread gives it the readable stream of a MediaStreamTrackProcessor, and tells it each ack, each request
 * for a key frame, and if the transport has room. The worker posts each encoded picture back, and the main thread
 * sends it. Thus the main thread, which handles the input of the desktop, does no camera work.
 */

import { CameraEncoder, type CameraFormat } from "./encoder";

/** A message of the main thread. */
export type ToWorker =
  | { type: "start"; readable: ReadableStream<VideoFrame>; config: VideoEncoderConfig; software: boolean; format: CameraFormat }
  | { type: "ack" }
  | { type: "key-frame" }
  | { type: "dropped" }
  | { type: "sendable"; value: boolean }
  | { type: "stop" };

/** A message of the worker. */
export type FromWorker = { type: "picture"; key: boolean; data: ArrayBuffer } | { type: "rate"; bitrate: number; frameRate: number };

const scope = self as unknown as {
  onmessage: ((event: MessageEvent<ToWorker>) => void) | null;
  postMessage(message: FromWorker, transfer?: Transferable[]): void;
};

let encoder: CameraEncoder | null = null;
let reader: ReadableStreamDefaultReader<VideoFrame> | null = null;
let sendable = true;
let last = { bitrate: 0, frameRate: 0 };

async function read(stream: ReadableStreamDefaultReader<VideoFrame>): Promise<void> {
  for (;;) {
    const { value: frame, done } = await stream.read().catch(() => ({ value: undefined, done: true }));
    if (done || !frame) return;
    const rect = frame.visibleRect;
    if (encoder && encoder.offer(frame, rect?.width ?? frame.displayWidth, rect?.height ?? frame.displayHeight, sendable)) {
      const rate = encoder.rate;
      if (rate.bitrate !== last.bitrate || rate.frameRate !== last.frameRate) {
        last = rate;
        scope.postMessage({ type: "rate", ...rate });
      }
    }
    frame.close();
  }
}

scope.onmessage = (event) => {
  const message = event.data;
  switch (message.type) {
    case "start":
      encoder = new CameraEncoder(message.config, message.software, message.format, (picture) => {
        scope.postMessage({ type: "picture", ...picture }, [picture.data]);
      });
      reader = message.readable.getReader();
      void read(reader);
      break;
    case "ack":
      encoder?.ack();
      break;
    case "key-frame":
      encoder?.keyFrame();
      break;
    case "dropped":
      encoder?.dropped();
      break;
    case "sendable":
      sendable = message.value;
      break;
    case "stop":
      void reader?.cancel().catch(() => {});
      encoder?.close();
      encoder = null;
      reader = null;
      break;
  }
};
