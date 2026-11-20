// page.js - the part of the farglass extension in the world of the page (F040)
//
// It wraps the Media Source Extensions of the page: each SourceBuffer, each
// append, remove, and abort, the duration, and the end of the stream. Each
// 250 ms, and at each play, pause, seek, and change of rate, it sends the
// state of each video element that plays a media source. The page plays as
// before. While the client shows a video, the host draws it black, thus the
// client sees it through the window and the host has nothing to encode.
//
// Messages out (window.postMessage, to relay.js): { farglassRedirect: message }.
// Messages in: { farglassRedirectBack: { type: "showing", media, showing } }.

(() => {
  if (window.__farglassRedirect || typeof MediaSource === "undefined") return;
  window.__farglassRedirect = true;

  const page = Math.random().toString(36).slice(2, 8);
  let count = 0;
  const mediaIds = new WeakMap();
  const sources = new WeakMap();
  const sourceCount = new WeakMap();
  const urls = new Map();
  /** The video element of each media source that plays. */
  const videos = new Map();
  const shown = new Set();

  const post = (message, transfer = []) => window.postMessage({ farglassRedirect: message }, "*", transfer);
  const idOf = (mediaSource) => {
    let id = mediaIds.get(mediaSource);
    if (!id) {
      id = `${page}-${++count}`;
      mediaIds.set(mediaSource, id);
    }
    return id;
  };
  const wrap = (proto, name, before) => {
    const original = proto[name];
    if (typeof original !== "function") return;
    proto[name] = function (...args) {
      try {
        before(this, ...args);
      } catch {
        // The page plays also if the copy fails.
      }
      return original.apply(this, args);
    };
  };

  const addSourceBuffer = MediaSource.prototype.addSourceBuffer;
  MediaSource.prototype.addSourceBuffer = function (mime) {
    const buffer = addSourceBuffer.call(this, mime);
    const media = idOf(this);
    const index = (sourceCount.get(this) ?? 0) + 1;
    sourceCount.set(this, index);
    const source = `${media}.${index}`;
    sources.set(buffer, { media, source });
    post({ type: "source", media, source, mime: String(mime) });
    return buffer;
  };
  wrap(SourceBuffer.prototype, "appendBuffer", (buffer, data) => {
    const ids = sources.get(buffer);
    if (!ids) return;
    const bytes = data instanceof ArrayBuffer ? data.slice(0) : data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);
    post({ type: "append", ...ids, offset: buffer.timestampOffset, data: bytes }, [bytes]);
  });
  wrap(SourceBuffer.prototype, "remove", (buffer, start, end) => {
    const ids = sources.get(buffer);
    if (ids) post({ type: "remove", ...ids, start, end: Number.isFinite(end) ? end : null });
  });
  wrap(SourceBuffer.prototype, "abort", (buffer) => {
    const ids = sources.get(buffer);
    if (ids) post({ type: "abort", ...ids });
  });
  wrap(MediaSource.prototype, "endOfStream", (mediaSource) => post({ type: "end", media: idOf(mediaSource) }));
  const duration = Object.getOwnPropertyDescriptor(MediaSource.prototype, "duration");
  if (duration?.set) {
    Object.defineProperty(MediaSource.prototype, "duration", {
      ...duration,
      set(value) {
        duration.set.call(this, value);
        post({ type: "duration", media: idOf(this), value: Number.isFinite(value) ? value : null });
      },
    });
  }
  const createObjectURL = URL.createObjectURL;
  URL.createObjectURL = function (object) {
    const url = createObjectURL.call(this, object);
    if (object instanceof MediaSource) urls.set(url, object);
    return url;
  };

  /** Reports whether the page under the video is black: the first background under it is black. */
  const blackBehind = (video) => {
    for (let element = video.parentElement; element; element = element.parentElement) {
      const color = getComputedStyle(element).backgroundColor;
      const match = /rgba?\(([\d.]+), ([\d.]+), ([\d.]+)(?:, ([\d.]+))?\)/.exec(color);
      if (!match || match[4] === "0") continue;
      return match[1] === "0" && match[2] === "0" && match[3] === "0";
    }
    return false;
  };

  /**
   * Sets how the host draws a video that the client shows: black, so that the client sees through it.
   * A hidden video costs the host nothing to draw and gives no new frames to encode, thus the
   * video hides if the page under it is black, as in most players. Else a filter draws it black.
   */
  const draw = (video, black) => {
    if (black) {
      const property = blackBehind(video) ? "visibility" : "filter";
      if (video.dataset.farglassProperty === property) return;
      // The page changed under the video, for example after its layout: the other way applies now.
      if ("farglassProperty" in video.dataset) draw(video, false);
      video.dataset.farglassProperty = property;
      video.dataset.farglassValue = video.style.getPropertyValue(property);
      video.style.setProperty(property, property === "visibility" ? "hidden" : "brightness(0)", "important");
    } else if ("farglassProperty" in video.dataset) {
      video.style.setProperty(video.dataset.farglassProperty, video.dataset.farglassValue);
      delete video.dataset.farglassProperty;
      delete video.dataset.farglassValue;
    }
  };

  const state = (media, video) => {
    const rect = video.getBoundingClientRect();
    const zoom = window.devicePixelRatio || 1;
    // The viewport in the window, in logical pixels. The frame of the browser has the same width
    // on the left, the right, and the bottom, and the tabs and the toolbar above.
    const width = window.innerWidth * zoom;
    const height = window.innerHeight * zoom;
    const side = Math.max(0, (window.outerWidth - width) / 2);
    const viewport = { x: side, y: Math.max(0, window.outerHeight - height - side), width, height };
    const inView = rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.bottom > 0 && rect.left < window.innerWidth && rect.top < window.innerHeight;
    post({
      type: "state",
      media,
      time: video.currentTime,
      paused: video.paused,
      rate: video.playbackRate,
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      viewport,
      zoom,
      title: document.title,
      visible: document.visibilityState === "visible" && inView,
    });
  };

  const scan = () => {
    const seen = new Set();
    for (const video of document.querySelectorAll("video")) {
      const mediaSource = urls.get(video.currentSrc || video.src);
      if (!mediaSource) continue;
      const media = idOf(mediaSource);
      seen.add(media);
      if (videos.get(media) !== video) {
        videos.set(media, video);
        for (const name of ["play", "pause", "seeked", "seeking", "ratechange", "playing", "waiting"]) {
          video.addEventListener(name, () => videos.get(media) === video && state(media, video));
        }
      }
      state(media, video);
      if (shown.has(media)) draw(video, true);
    }
    for (const [media, video] of videos) {
      if (!seen.has(media)) {
        videos.delete(media);
        if (shown.delete(media)) draw(video, false);
        post({ type: "gone", media });
      }
    }
  };
  setInterval(scan, 250);
  document.addEventListener("visibilitychange", scan);
  window.addEventListener("scroll", scan, { passive: true });
  window.addEventListener("resize", scan);
  window.addEventListener("pagehide", () => {
    for (const media of videos.keys()) post({ type: "gone", media });
  });

  window.addEventListener("message", (event) => {
    const message = event.source === window ? event.data?.farglassRedirectBack : undefined;
    if (message?.type !== "showing") return;
    const video = videos.get(message.media);
    if (!video) return;
    if (message.showing) shown.add(message.media);
    else shown.delete(message.media);
    draw(video, message.showing);
  });
})();
