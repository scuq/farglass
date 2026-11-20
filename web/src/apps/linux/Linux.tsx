import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import type { Connection, SurfaceRect } from "../../proto";
import { isPasteKey } from "../../surface/clipboard";
import { IS_MAC, linuxKey, MODIFIER_KEYS } from "../../surface/keys";
import { type SurfaceInfo, type SurfaceManager, surfaceManagerFor, surfaceOf } from "../../surface/manager";
import type { AppContext } from "../types";
import "./linux.css";

/**
 * Shows one surface on a canvas: a worker decodes the tile stream of a surface
 * channel and draws on an OffscreenCanvas. Returns the canvas element.
 */
function SurfaceCanvas(props: {
  connection: Connection;
  manager: SurfaceManager;
  id: number;
  left: number;
  top: number;
  scale: number;
  onpress: (id: number) => boolean;
  /** Reports whether the shell moves the window now: then the app gets no pointer input. */
  moving: () => boolean;
  onpressed: (press: { pointerId: number; clientX: number; clientY: number; canvas: HTMLCanvasElement } | null) => void;
  /** The rectangle in buffer pixels where black is transparent: a redirected video shows through (F040). */
  keyRect?: () => { x: number; y: number; width: number; height: number } | null;
}) {
  const canvas = document.createElement("canvas");
  canvas.dataset.surface = String(props.id);
  // The pointer input: the coordinates are logical pixels of this surface.
  const at = (event: PointerEvent | WheelEvent) => {
    const rect = canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };
  canvas.addEventListener("pointermove", (event) => {
    if (props.moving()) return;
    props.manager.input({ type: "pointer", id: props.id, ...at(event) });
  });
  canvas.addEventListener("pointerdown", (event) => {
    // A press outside the open popups closes them, and the press goes no further.
    if (!props.onpress(props.id)) {
      event.preventDefault();
      return;
    }
    canvas.setPointerCapture(event.pointerId);
    props.onpressed({ pointerId: event.pointerId, clientX: event.clientX, clientY: event.clientY, canvas });
    props.manager.input({ type: "pointer", id: props.id, ...at(event) });
    props.manager.input({ type: "button", button: event.button, pressed: true });
  });
  canvas.addEventListener("pointerup", (event) => {
    props.onpressed(null);
    props.manager.input({ type: "button", button: event.button, pressed: false });
  });
  canvas.addEventListener("pointerleave", () => props.manager.input({ type: "leave" }));
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? 400 : 1;
      props.manager.input({ type: "pointer", id: props.id, ...at(event) });
      props.manager.input({ type: "axis", dx: event.deltaX * unit, dy: event.deltaY * unit });
    },
    { passive: false },
  );
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  const worker = new Worker(new URL("../../surface/worker.ts", import.meta.url), { type: "module" });
  const offscreen = canvas.transferControlToOffscreen();
  worker.postMessage({ canvas: offscreen }, [offscreen]);
  worker.onmessage = (event: MessageEvent<{ type: string; width?: number; height?: number; seq?: number; frames?: number }>) => {
    // The count of video frames: a hint for diagnosis and for the tests (F037).
    if (event.data.type === "video") {
      canvas.dataset.videoFrames = String(event.data.frames ?? 0);
    }
    if (event.data.type === "video-failed") {
      canvas.dataset.video = "failed";
      channel.control({ type: "no-video" });
    }
    // The acknowledgment of each frame lets the agent send the next one (F029).
    if (event.data.type === "end") {
      channel.control({ type: "ack", seq: event.data.seq ?? 0 });
    }
    if (event.data.type === "size") {
      canvas.style.width = `${(event.data.width ?? 0) / props.scale}px`;
      canvas.style.height = `${(event.data.height ?? 0) / props.scale}px`;
    }
  };
  const channel = props.connection.openChannel(
    // The video of a region needs WebCodecs in the worker (F037). With none, the agent sends tiles only.
    { kind: "surface", id: props.id, video: typeof VideoDecoder !== "undefined" },
    {
      ondata: (data) => {
        const copy = data.slice().buffer;
        worker.postMessage({ bytes: copy }, [copy]);
      },
    },
  );
  createEffect(
    () => JSON.stringify(props.keyRect?.() ?? null),
    (key) => {
      const rect = JSON.parse(key) as { x: number; y: number; width: number; height: number } | null;
      worker.postMessage({ key: rect });
      // The encoder of the agent sends this rectangle as tiles, never as video: the key needs their exact black (F040).
      channel.control(rect ? { type: "key", rect } : { type: "key" });
    },
  );
  createEffect(
    () => [props.left, props.top] as const,
    ([left, top]) => {
      canvas.style.left = `${left}px`;
      canvas.style.top = `${top}px`;
    },
  );
  onCleanup(() => {
    channel.close();
    worker.terminate();
  });
  return canvas;
}

const origin = (geometry: SurfaceRect | null) => geometry ?? { x: 0, y: 0, width: 0, height: 0 };

/** A window of a Wayland app: its top-level surface and its popups. */
export default function Linux(props: { context: AppContext }) {
  const id = surfaceOf(props.context.windowId);
  const manager = surfaceManagerFor(props.context.connection);
  if (!manager) {
    return <p class="linux-waiting">The surfaces of this session are not available.</p>;
  }
  const info = () => manager.surfaces().get(id);
  let element: HTMLDivElement | undefined;
  /** The container of the redirected videos. Solid does not manage its children, thus the canvases never move in the page. */
  let redirects: HTMLDivElement | undefined;
  const [size, setSize] = createSignal<{ w: number; h: number } | null>(null);

  // The cursor that the app selected shows on the window: the browser draws it with no delay.
  createEffect(
    () => manager.cursor(),
    (cursor) => {
      if (element) element.style.cursor = cursor;
    },
  );
  createEffect(
    () => info()?.title,
    (title) => {
      if (title) {
        props.context.setTitle(title);
      }
    },
  );
  props.context.onFocus(() => {
    manager.request({ type: "activate", id });
    element?.focus({ preventScroll: true });
  });

  // The keyboard: each key goes as its Linux key code to the window with the focus.
  // While the clipboard of the browser goes to the apps for a paste, the keys wait in order (F030).
  const held = new Set<number>();
  let waiting = 0;
  let queue = Promise.resolve();
  const sendKey = (code: number, pressed: boolean) => {
    if (waiting === 0) {
      manager.input({ type: "key", id, code, pressed });
    } else {
      queue = queue.then(() => manager.input({ type: "key", id, code, pressed }));
    }
  };
  const onKey = (event: KeyboardEvent, pressed: boolean) => {
    const code = linuxKey(event.code);
    if (code === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    if (pressed && isPasteKey(event, IS_MAC)) {
      waiting += 1;
      const sync = manager.clipboard.sync();
      queue = queue.then(() => sync).then(() => {
        waiting -= 1;
      });
    }
    // A Mac browser gives no key up for a key that goes up while Cmd is down. Thus such a key is a tap,
    // and the app does not repeat it.
    if (IS_MAC && event.metaKey && !MODIFIER_KEYS.has(code)) {
      if (pressed) {
        sendKey(code, true);
        sendKey(code, false);
      }
      return;
    }
    if (pressed) held.add(code);
    else held.delete(code);
    sendKey(code, pressed);
  };
  // A key that is down when the window loses the focus goes up, as on a local seat.
  const releaseAll = () => {
    for (const code of held) sendKey(code, false);
    held.clear();
  };
  onCleanup(releaseAll);

  // A press that can become a move of the window: the app asks for the move after the press (F029).
  let press: { pointerId: number; clientX: number; clientY: number; canvas: HTMLCanvasElement } | null = null;
  let moving = false;
  const onpressed = (next: typeof press) => {
    press = next;
    if (!next) moving = false;
  };
  onCleanup(
    manager.onMoveRequest(id, () => {
      if (!press || moving) return;
      moving = true;
      press.canvas.dispatchEvent(
        new CustomEvent("farglass-move", { bubbles: true, detail: { pointerId: press.pointerId, clientX: press.clientX, clientY: press.clientY } }),
      );
    }),
  );

  /** A press on the surface `target`: closes each popup that is not `target` or under it. Returns `true` if the press goes to the app. */
  const pressAt = (target: number): boolean => {
    const open = popups();
    if (open.length === 0) return true;
    const keep = new Set<number>();
    for (let current: number | null = target; current !== null; ) {
      keep.add(current);
      current = manager.surfaces().get(current)?.parent ?? null;
    }
    const close = open.filter((popup) => !keep.has(popup.id));
    for (const popup of close) manager.request({ type: "popup-dismiss", id: popup.id });
    return close.length === 0;
  };
  props.context.onClose(() => manager.request({ type: "close", id }));

  // The videos of the host browser in this window play in the client (F040). The container has
  // the window geometry at (0, 0): a rectangle of the window is a rectangle of the container.
  const players = () => {
    const surface = info();
    const frame = surface?.geometry ?? (surface ? { width: surface.width / surface.scale, height: surface.height / surface.scale } : undefined);
    return manager.redirect.playersFor(surface?.title ?? "", frame && { width: frame.width, height: frame.height });
  };
  const shown = new Set<string>();
  createEffect(players, (list) => {
    const now = new Set<string>();
    for (const { media, rect } of list) {
      now.add(media.id);
      if (redirects && media.video.parentElement !== redirects) redirects.append(media.video);
      Object.assign(media.video.style, { left: `${rect.x}px`, top: `${rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` });
      manager.redirect.setShowing(media.id, true);
    }
    for (const id of shown) {
      if (!now.has(id)) {
        manager.redirect.setShowing(id, false);
        redirects?.querySelector(`video[data-redirect="${CSS.escape(id)}"]`)?.remove();
      }
    }
    shown.clear();
    for (const id of now) shown.add(id);
  });
  onCleanup(() => {
    for (const id of shown) manager.redirect.setShowing(id, false);
  });
  /** The key rectangle of the first player, in buffer pixels, inside its rectangle. */
  const keyRect = () => {
    const first = players()[0];
    const surface = info();
    if (!first || !surface) return null;
    const geometry = origin(surface.geometry);
    const scale = surface.scale;
    const x = Math.ceil((first.rect.x + geometry.x) * scale);
    const y = Math.ceil((first.rect.y + geometry.y) * scale);
    return {
      x,
      y,
      width: Math.floor((first.rect.x + first.rect.width + geometry.x) * scale) - x,
      height: Math.floor((first.rect.y + first.rect.height + geometry.y) * scale) - y,
    };
  };

  // The size of the content goes to the client, after a short quiet time.
  let timer: ReturnType<typeof setTimeout> | undefined;
  let sent = "";
  const observer = new ResizeObserver(([entry]) => {
    if (!entry) return;
    const w = Math.round(entry.contentRect.width);
    const h = Math.round(entry.contentRect.height);
    setSize({ w, h });
    clearTimeout(timer);
    timer = setTimeout(() => {
      const current = info()?.geometry;
      const key = `${w}x${h}`;
      if (key !== sent && (!current || current.width !== w || current.height !== h)) {
        sent = key;
        manager.request({ type: "configure", id, width: w, height: h });
      }
    }, 120);
  });
  onCleanup(() => {
    observer.disconnect();
    clearTimeout(timer);
  });

  /** The place of a popup in the window: the chain of parents to this window. */
  const placeOf = (popup: SurfaceInfo): { left: number; top: number } => {
    const parent = popup.parent === null ? undefined : manager.surfaces().get(popup.parent);
    if (!parent) return { left: 0, top: 0 };
    const base = parent.popup ? placeOf(parent) : { left: -origin(parent.geometry).x, top: -origin(parent.geometry).y };
    const parentGeometry = origin(parent.geometry);
    const place = origin(popup.place);
    const own = origin(popup.geometry);
    return { left: base.left + parentGeometry.x + place.x - own.x, top: base.top + parentGeometry.y + place.y - own.y };
  };
  /** Each popup of this window, also the popups of its popups. */
  const popups = () => {
    const all: SurfaceInfo[] = [];
    const walk = (parent: number) => {
      for (const popup of manager.popupsOf(parent)) {
        if (popup.mapped) {
          all.push(popup);
          walk(popup.id);
        }
      }
    };
    walk(id);
    return all;
  };

  return (
    <div
      class="linux-app"
      tabindex="0"
      ref={(el) => {
        element = el;
        observer.observe(el);
        el.addEventListener("keydown", (event) => onKey(event, true));
        el.addEventListener("keyup", (event) => onKey(event, false));
        el.addEventListener("blur", releaseAll);
        // A press on the window gives the keyboard to the app, as on a local desktop.
        el.addEventListener("pointerdown", () => el.focus({ preventScroll: true }));
        // The window can have the focus before the app module loaded: the focus hook came too late for it.
        queueMicrotask(() => {
          if (props.context.isFocused()) {
            manager.request({ type: "activate", id });
            el.focus({ preventScroll: true });
          }
        });
      }}
      data-surface={id}
      data-cursor={manager.cursor()}
      data-size={size() ? `${size()!.w}x${size()!.h}` : undefined}
    >
      {/* The window itself, cut at the frame. Its popups are outside: a menu can reach past the window, as on a desktop. */}
      <div class="linux-main">
        <div
          class="linux-redirects"
          ref={(el) => {
            redirects = el;
          }}
        />
        <Show when={info()} fallback={<p class="linux-waiting">The app is not connected.</p>}>
          {(surface) => (
            <SurfaceCanvas
              connection={props.context.connection}
              manager={manager}
              onpress={pressAt}
              moving={() => moving}
              onpressed={onpressed}
              keyRect={keyRect}
              id={id}
              left={-origin(surface().geometry).x}
              top={-origin(surface().geometry).y}
              scale={surface().scale}
            />
          )}
        </Show>
      </div>
      <For each={popups()} keyed={(popup: SurfaceInfo) => popup.id}>
        {(popup) => (
          <SurfaceCanvas
            connection={props.context.connection}
            manager={manager}
            onpress={pressAt}
            moving={() => moving}
            onpressed={onpressed}
            id={popup().id}
            left={placeOf(popup()).left}
            top={placeOf(popup()).top}
            scale={popup().scale}
          />
        )}
      </For>
    </div>
  );
}
