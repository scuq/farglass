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

/** The state of a video element of the host. */
type HostState = Extract<RedirectEvent, { type: "state" }>;

/** Returns the rectangle of the video in the window, in logical pixels, clipped to the viewport. None if no part shows. */
export function placement(state: Pick<HostState, "rect" | "viewport" | "zoom">): RedirectRect | null {
  const { rect, viewport, zoom } = state;
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

/** Reports whether the client must seek: its time is too far from the time of the host. */
export function mustSeek(clientTime: number, host: number): boolean {
  return Math.abs(clientTime - host) > SEEK_AFTER;
}

type Operation =
  | { readonly kind: "append"; readonly data: Uint8Array<ArrayBuffer>; readonly offset: number }
  | { readonly kind: "remove"; readonly start: number; readonly end: number }
  | { readonly kind: "abort" };

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
  /** Reports whether the video had a frame. A seek then does not take it away. */
  started = false;
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

  /** Follows the time, the pause, and the rate of the host. */
  sync(now: number): void {
    const state = this.state;
    if (!state || this.failed) return;
    const video = this.video;
    if (video.playbackRate !== state.rate) video.playbackRate = state.rate;
    const host = hostTime(state, now - this.stateAt);
    if (video.readyState >= HTMLMediaElement.HAVE_METADATA && !video.seeking && mustSeek(video.currentTime, host)) {
      video.currentTime = host;
    }
    if (state.paused && !video.paused) video.pause();
    if (!state.paused && video.paused) void video.play().catch((error: unknown) => console.debug("video redirection: no play", error));
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

export class RedirectManager {
  readonly #channel: Channel;
  readonly #medias = new Map<string, RedirectedMedia>();
  readonly #version = createSignal(0, { equals: false });
  readonly #showing = new Map<string, boolean>();
  #append: { media: string; source: string; offset: number; data: Uint8Array<ArrayBuffer>; got: number } | null = null;
  readonly #timer: ReturnType<typeof setInterval>;

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "redirect" },
      { oncontrol: (message) => this.#event(message as RedirectEvent), ondata: (data) => this.#data(data) },
    );
    this.#timer = setInterval(() => this.#sync(), 250);
  }

  close(): void {
    clearInterval(this.#timer);
    this.#channel.close();
    for (const media of this.#medias.values()) media.close();
    this.#medias.clear();
  }

  /** The players that show in a window whose title is `title`. */
  playersFor(title: string): Player[] {
    this.#version[0]();
    const players: Player[] = [];
    for (const media of this.#medias.values()) {
      const state = media.state;
      if (!state || !state.visible || media.failed || !state.title || !title.startsWith(state.title)) continue;
      if (media.video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) media.started = true;
      if (!media.started) continue;
      const rect = placement(state);
      if (rect) players.push({ media, rect });
    }
    return players;
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
