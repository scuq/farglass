/**
 * Video redirection (F040): the videos of the host browser play in the client browser.
 *
 * The farglass extension in the host browser copies what a page gives to Media
 * Source Extensions. For each media source of the host, the manager makes a
 * MediaSource and a video element here, and appends the same data. It keeps
 * the time, the pause, and the rate as on the host. The window of the host
 * browser shows the video element under its canvas: the host draws the video
 * black, and the worker makes the black of that rectangle transparent. Thus
 * the controls of the page stay on top, and the client decodes the video.
 */

import { createSignal } from "solid-js";
import type { Channel, Connection, RedirectEvent, RedirectRect, RedirectRequest } from "../proto";

/** The longest difference of the time from the host before the client seeks, in seconds. */
const SEEK_AFTER = 0.4;

/**
 * In the smart mode (F073): the time with no pointer movement on a playing video after which
 * the host stops to draw it, and the client shows its own video again.
 */
export const SMART_IDLE_MS = 3000;

/** A difference of the time from the host that the client does not correct, in seconds. */
const IN_STEP = 0.03;

/** The largest change of the rate that brings the client back to the time of the host. */
const MAX_NUDGE = 0.1;

/** The time with no first frame after which the client gives the video back to the host. */
export const FIRST_FRAME_MS = 8000;

/** The time with no progress while the host plays after which the client gives the video back to the host. */
export const STALL_MS = 3000;

/** The state of a video element of the host. */
type HostState = Extract<RedirectEvent, { type: "state" }>;

/**
 * Returns the rectangle of the video in the window, in logical pixels, clipped to the viewport. None if no part shows.
 *
 * With `frame`, the size of the window geometry from the compositor, the viewport goes at the bottom
 * of the window, with side frames of the same width. The page knows the size of its viewport, but its
 * outer size can have the shadow of the window in it.
 */
export function placement(state: Pick<HostState, "rect" | "viewport" | "zoom">, frame?: { width: number; height: number }): RedirectRect | null {
  const { rect, zoom } = state;
  let viewport = state.viewport;
  if (frame && frame.width >= viewport.width && frame.height >= viewport.height) {
    const side = (frame.width - viewport.width) / 2;
    viewport = { ...viewport, x: side, y: Math.max(0, frame.height - viewport.height - side) };
  }
  const x0 = Math.max(viewport.x + rect.x * zoom, viewport.x);
  const y0 = Math.max(viewport.y + rect.y * zoom, viewport.y);
  const x1 = Math.min(viewport.x + (rect.x + rect.width) * zoom, viewport.x + viewport.width);
  const y1 = Math.min(viewport.y + (rect.y + rect.height) * zoom, viewport.y + viewport.height);
  if (x1 - x0 < 2 || y1 - y0 < 2) return null;
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** Returns the time of the host now, from its last state that came `elapsed` ms ago. */
export function hostTime(state: Pick<HostState, "time" | "paused" | "rate">, elapsed: number): number {
  return state.paused ? state.time : state.time + (elapsed / 1000) * state.rate;
}

/**
 * Returns the rate for the client: the rate of the host, a little faster or slower while the client
 * is `drift` seconds behind (positive) or ahead (negative) of the host. Thus a change between the
 * video of the client and the video of the host shows no jump (F073).
 */
export function stepRate(rate: number, drift: number): number {
  if (Math.abs(drift) < IN_STEP) return rate;
  return rate * (1 + Math.max(-MAX_NUDGE, Math.min(MAX_NUDGE, drift / 2)));
}

/**
 * Reports whether the host draws the video `media` in the smart mode: the pointer is on it, and it moved
 * less than {@link SMART_IDLE_MS} ago, or the video is paused (F073).
 */
export function smartHostDraws(pointer: { readonly media: string; readonly at: number } | null, media: string, now: number, paused: boolean): boolean {
  return pointer !== null && pointer.media === media && (paused || now - pointer.at < SMART_IDLE_MS);
}

/** Reports whether the client must seek: its time is too far from the time of the host. */
export function mustSeek(clientTime: number, host: number): boolean {
  return Math.abs(clientTime - host) > SEEK_AFTER;
}

type Operation =
  | { readonly kind: "append"; readonly data: Uint8Array<ArrayBuffer>; readonly offset: number }
  | { readonly kind: "remove"; readonly start: number; readonly end: number }
  | { readonly kind: "abort" }
  | { readonly kind: "change-type"; readonly mime: string };

/** One SourceBuffer of the host and its copy here. */
class Source {
  buffer: SourceBuffer | null = null;
  readonly queue: Operation[] = [];
  constructor(readonly mime: string) {}
}

/** One media source of the host: a MediaSource and a video element here. */
export class RedirectedMedia {
  readonly video: HTMLVideoElement;
  readonly #mediaSource = new MediaSource();
  readonly #url: string;
  readonly sources = new Map<string, Source>();
  state: HostState | null = null;
  stateAt = 0;
  failed = false;
  /** Reports whether the video had a frame. */
  started = false;
  /**
   * Reports whether the video played in step with the host one time. Only then does the client
   * show it, thus the host does not hide its video while the client still seeks. A later seek keeps it.
   */
  inStep = false;
  /** Why the client gave the video back to the host, for the diagnosis. */
  reason: string | null = null;
  readonly #created = performance.now();
  #lastTime = -1;
  #lastProgress = performance.now();
  #duration: number | null | undefined;
  #ended = false;

  constructor(readonly id: string) {
    this.video = document.createElement("video");
    this.video.className = "linux-redirect";
    this.video.dataset.redirect = id;
    // The sound of the host goes through the sound of the session (F038).
    this.video.muted = true;
    this.video.playsInline = true;
    this.#url = URL.createObjectURL(this.#mediaSource);
    this.video.src = this.#url;
    this.#mediaSource.addEventListener("sourceopen", () => {
      for (const source of this.sources.values()) this.#open(source);
    });
  }

  #open(source: Source): void {
    if (source.buffer || this.failed || this.#mediaSource.readyState !== "open") return;
    if (!MediaSource.isTypeSupported(source.mime)) {
      console.warn("video redirection: this browser cannot play", source.mime);
      this.failed = true;
      return;
    }
    try {
      source.buffer = this.#mediaSource.addSourceBuffer(source.mime);
    } catch (error) {
      console.warn("video redirection", error);
      this.failed = true;
      return;
    }
    source.buffer.addEventListener("updateend", () => this.pump());
    this.pump();
  }

  addSource(id: string, mime: string): void {
    const source = new Source(mime);
    this.sources.set(id, source);
    this.#open(source);
  }

  push(id: string, operation: Operation): void {
    const source = this.sources.get(id);
    if (!source) return;
    source.queue.push(operation);
    this.pump();
  }

  setDuration(value: number | null): void {
    this.#duration = value;
    this.pump();
  }

  end(): void {
    this.#ended = true;
    this.pump();
  }

  /** Runs the next operation of each idle SourceBuffer, then the duration and the end. */
  pump(): void {
    if (this.failed || this.#mediaSource.readyState === "closed") return;
    for (const source of this.sources.values()) {
      const buffer = source.buffer;
      if (!buffer || buffer.updating) continue;
      const next = source.queue.shift();
      if (!next) continue;
      try {
        if (next.kind === "append") {
          if (buffer.timestampOffset !== next.offset) buffer.timestampOffset = next.offset;
          buffer.appendBuffer(next.data);
        } else if (next.kind === "remove") {
          buffer.remove(next.start, Math.min(next.end, this.#mediaSource.duration || next.end));
        } else if (next.kind === "change-type") {
          if (!MediaSource.isTypeSupported(next.mime)) {
            this.fail(`this browser cannot play ${next.mime}`);
            return;
          }
          buffer.changeType(next.mime);
          this.pump();
        } else {
          buffer.abort();
          this.pump();
        }
      } catch (error) {
        if (error instanceof DOMException && error.name === "QuotaExceededError" && next.kind === "append") {
          // The buffer is full: the old part goes first, and the append tries again.
          source.queue.unshift(next);
          const before = this.video.currentTime - 5;
          if (before > 0) buffer.remove(0, before);
        } else {
          console.warn("video redirection", error);
        }
      }
    }
    const idle = [...this.sources.values()].every((source) => source.buffer && !source.buffer.updating && source.queue.length === 0);
    if (!idle || this.#mediaSource.readyState !== "open") return;
    if (this.#duration !== undefined) {
      try {
        this.#mediaSource.duration = this.#duration ?? Infinity;
      } catch {
        // A duration below the buffered end: the next append sets it again.
      }
      this.#duration = undefined;
    }
    if (this.#ended) {
      this.#ended = false;
      this.#mediaSource.endOfStream();
    }
  }

  /** Gives the video back to the host: the client no longer shows it, and the host draws it again. */
  fail(reason: string): void {
    if (this.failed) return;
    this.failed = true;
    this.reason = reason;
    console.warn(`video redirection: the host draws ${this.id} again: ${reason}`);
  }

  /** Checks that the video plays: no error, a first frame in time, and progress while the host plays. */
  #check(now: number, hostPlays: boolean): void {
    const video = this.video;
    if (video.error) {
      this.fail(`the decoder of this browser failed: ${video.error.message || video.error.code}`);
      return;
    }
    if (Math.abs(video.currentTime - this.#lastTime) > 0.05 || video.seeking || !hostPlays) {
      this.#lastTime = video.currentTime;
      this.#lastProgress = now;
      return;
    }
    if (!this.started && now - this.#created > FIRST_FRAME_MS) {
      this.fail("no first frame");
    } else if (this.started && now - this.#lastProgress > STALL_MS) {
      this.fail("the video stopped while the host plays");
    }
  }

  /** Follows the time, the pause, and the rate of the host. */
  sync(now: number): void {
    const state = this.state;
    if (!state || this.failed) return;
    const video = this.video;
    if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) this.started = true;
    this.#check(now, !state.paused);
    if (this.failed) return;
    if (!this.inStep && video.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA && !video.seeking) {
      this.inStep = Math.abs(video.currentTime - hostTime(state, now - this.stateAt)) < SEEK_AFTER * 1.5;
    }
    const host = hostTime(state, now - this.stateAt);
    const rate = state.paused || video.seeking ? state.rate : stepRate(state.rate, host - video.currentTime);
    if (video.playbackRate !== rate) video.playbackRate = rate;
    if (video.readyState >= HTMLMediaElement.HAVE_METADATA && !video.seeking && mustSeek(video.currentTime, host)) {
      video.currentTime = host;
    }
    if (state.paused && !video.paused) video.pause();
    if (!state.paused && video.paused) {
      // A browser can refuse to play: the health check then gives the video back to the host.
      video.play()?.catch((error: unknown) => console.debug("video redirection: no play", error));
    }
    video.dataset.hostTime = host.toFixed(2);
    video.dataset.hostPaused = String(state.paused);
  }

  close(): void {
    this.video.remove();
    this.video.removeAttribute("src");
    URL.revokeObjectURL(this.#url);
  }
}

/** A redirected video for a window of the host browser. */
export interface Player {
  readonly media: RedirectedMedia;
  /** The rectangle in the window, in logical pixels. */
  readonly rect: RedirectRect;
}

/** The key of the setting in the state store: where the videos of the host browser play. */
export const VIDEO_KEY = "video-redirect";

/**
 * Where the videos of the host browser play: in the client browser (redirection), on the host,
 * or smart: in the client, and on the host while the pointer is on the video (F073).
 */
export type VideoMode = "client" | "host" | "smart";

/** Returns the mode from a value of the state store. The default is smart (F074). */
export function parseVideoMode(value: unknown): VideoMode {
  const mode = typeof value === "object" && value !== null ? (value as { mode?: unknown }).mode : undefined;
  return mode === "host" || mode === "client" ? mode : "smart";
}

export class RedirectManager {
  readonly #channel: Channel;
  readonly #mode = createSignal<VideoMode>("smart");
  readonly #medias = new Map<string, RedirectedMedia>();
  readonly #version = createSignal(0, { equals: false });
  readonly #showing = new Map<string, boolean>();
  /** The video under the pointer and the time of the last pointer movement on it (F073). */
  #pointer: { media: string; at: number } | null = null;
  #append: { media: string; source: string; offset: number; data: Uint8Array<ArrayBuffer>; got: number } | null = null;
  readonly #timer: ReturnType<typeof setInterval>;

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "redirect" },
      { oncontrol: (message) => this.#event(message as RedirectEvent), ondata: (data) => this.#data(data) },
    );
    this.#timer = setInterval(() => this.#sync(), 250);
    // A diagnosis in the console of the client: farglassRedirect() lists each video and its state.
    (globalThis as { farglassRedirect?: () => unknown }).farglassRedirect = () => this.diagnose();
    // farglassVideoMode("smart") sets the mode of this tab only, for a test or a measurement (F073).
    (globalThis as { farglassVideoMode?: (mode: VideoMode) => void }).farglassVideoMode = (mode) => this.setMode(mode);
  }

  /** Returns the state of each redirected video, for the diagnosis in the console. */
  diagnose(): unknown[] {
    return [...this.#medias.values()].map((media) => {
      const video = media.video;
      return {
        id: media.id,
        title: media.state?.title,
        showing: this.#showing.get(media.id) ?? false,
        hostDraws: this.hostDraws(media.id),
        failed: media.reason,
        started: media.started,
        inStep: media.inStep,
        paused: video.paused,
        time: Number(video.currentTime.toFixed(2)),
        host: media.state ? Number(hostTime(media.state, performance.now() - media.stateAt).toFixed(2)) : null,
        ready: video.readyState,
        size: `${video.videoWidth}x${video.videoHeight}`,
        error: video.error?.message ?? null,
        sources: [...media.sources.values()].map((source) => ({ mime: source.mime, queued: source.queue.length, buffered: source.buffer?.buffered?.length ?? null })),
      };
    });
  }

  close(): void {
    clearInterval(this.#timer);
    this.#channel.close();
    for (const media of this.#medias.values()) media.close();
    this.#medias.clear();
  }

  /** Sets where the videos play. With "host", the client shows no video, and the host draws each one again. */
  setMode(mode: VideoMode): void {
    this.#mode[1](mode);
  }

  /**
   * The players that show in the window `window` with the title `title`, and whose window geometry has the size `frame`.
   *
   * A video goes only on a window of the browser process that sent it, and only if the title of the page is
   * the window title with no browser name (F052).
   */
  playersFor(window: number, title: string, frame?: { width: number; height: number }): Player[] {
    this.#version[0]();
    if (this.#mode[0]() === "host") return [];
    const players: Player[] = [];
    for (const media of this.#medias.values()) {
      const state = media.state;
      if (!state || !state.visible || media.failed || !state.title) continue;
      if (!(state.windows ?? []).includes(window) || !sameTitle(title, state.title)) continue;
      if (!media.started || !media.inStep) continue;
      const rect = placement(state, frame);
      if (rect) players.push({ media, rect });
    }
    return players;
  }

  /**
   * Takes the video under the pointer of the user, or null when the pointer is on no video.
   * Each call is a pointer movement. In the smart mode, the host then draws that video (F073).
   */
  pointer(media: string | null): void {
    const before = this.#pointer?.media ?? null;
    this.#pointer = media === null ? null : { media, at: performance.now() };
    if (before !== media) this.#changed();
  }

  /** Reports whether the host draws the video `media` now: only in the smart mode, while the user uses it (F073). */
  hostDraws(media: string): boolean {
    this.#version[0]();
    if (this.#mode[0]() !== "smart") return false;
    const paused = this.#medias.get(media)?.state?.paused ?? false;
    return smartHostDraws(this.#pointer, media, performance.now(), paused);
  }

  /** Tells the agent if the client shows a media source: the host then draws it black. */
  setShowing(media: string, showing: boolean): void {
    if ((this.#showing.get(media) ?? false) === showing) return;
    this.#showing.set(media, showing);
    this.#channel.control({ type: "showing", media, showing } satisfies RedirectRequest);
  }

  #changed(): void {
    this.#version[1](0);
  }

  #sync(): void {
    const now = performance.now();
    for (const media of this.#medias.values()) media.sync(now);
    // The video element gets its first frame, or the time moves: the players change.
    this.#changed();
  }

  #event(event: RedirectEvent): void {
    switch (event.type) {
      case "source": {
        let media = this.#medias.get(event.media);
        if (!media) {
          media = new RedirectedMedia(event.media);
          this.#medias.set(event.media, media);
        }
        media.addSource(event.source, event.mime);
        break;
      }
      case "append":
        this.#append = { media: event.media, source: event.source, offset: event.offset, data: new Uint8Array(event.size), got: 0 };
        if (event.size === 0) this.#appendDone();
        break;
      case "remove":
        this.#medias.get(event.media)?.push(event.source, { kind: "remove", start: event.start, end: event.end });
        break;
      case "change-type":
        this.#medias.get(event.media)?.push(event.source, { kind: "change-type", mime: event.mime });
        break;
      case "abort":
        this.#medias.get(event.media)?.push(event.source, { kind: "abort" });
        break;
      case "duration":
        this.#medias.get(event.media)?.setDuration(event.value ?? null);
        break;
      case "end":
        this.#medias.get(event.media)?.end();
        break;
      case "state": {
        const media = this.#medias.get(event.media);
        if (media) {
          media.state = event;
          media.stateAt = performance.now();
          media.sync(media.stateAt);
          this.#changed();
        }
        break;
      }
      case "gone": {
        this.#medias.get(event.media)?.close();
        this.#medias.delete(event.media);
        this.#showing.delete(event.media);
        this.#changed();
        break;
      }
    }
  }

  #data(data: Uint8Array): void {
    const append = this.#append;
    if (!append) return;
    const take = Math.min(data.length, append.data.length - append.got);
    append.data.set(data.subarray(0, take), append.got);
    append.got += take;
    if (append.got >= append.data.length) this.#appendDone();
  }

  #appendDone(): void {
    const append = this.#append;
    this.#append = null;
    if (append) this.#medias.get(append.media)?.push(append.source, { kind: "append", data: append.data, offset: append.offset });
  }
}

/**
 * Reports whether the window title `window` is the page title `page`: the same text, or the same text
 * and the name of the browser after " - " or " — ", for example "Video - Chromium".
 */
export function sameTitle(window: string, page: string): boolean {
  if (window === page) return true;
  for (const separator of [" - ", " — "]) {
    if (window.startsWith(page + separator) && !window.slice(page.length + separator.length).includes(separator)) return true;
  }
  return false;
}
