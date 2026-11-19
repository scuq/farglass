import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import type { Connection, SurfaceRect } from "../../proto";
import { type SurfaceInfo, surfaceManagerFor, surfaceOf } from "../../surface/manager";
import type { AppContext } from "../types";
import "./linux.css";

/**
 * Shows one surface on a canvas: a worker decodes the tile stream of a surface
 * channel and draws on an OffscreenCanvas. Returns the canvas element.
 */
function SurfaceCanvas(props: { connection: Connection; id: number; left: number; top: number; scale: number }) {
  const canvas = document.createElement("canvas");
  const worker = new Worker(new URL("../../surface/worker.ts", import.meta.url), { type: "module" });
  const offscreen = canvas.transferControlToOffscreen();
  worker.postMessage({ canvas: offscreen }, [offscreen]);
  worker.onmessage = (event: MessageEvent<{ type: string; width?: number; height?: number }>) => {
    if (event.data.type === "size") {
      canvas.style.width = `${(event.data.width ?? 0) / props.scale}px`;
      canvas.style.height = `${(event.data.height ?? 0) / props.scale}px`;
    }
  };
  const channel = props.connection.openChannel(
    { kind: "surface", id: props.id },
    {
      ondata: (data) => {
        const copy = data.slice().buffer;
        worker.postMessage({ bytes: copy }, [copy]);
      },
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
  const [size, setSize] = createSignal<{ w: number; h: number } | null>(null);

  createEffect(
    () => info()?.title,
    (title) => {
      if (title) {
        props.context.setTitle(title);
      }
    },
  );
  props.context.onFocus(() => manager.request({ type: "activate", id }));
  props.context.onClose(() => manager.request({ type: "close", id }));

  // The size of the content goes to the client, after a short quiet time.
  let element: HTMLDivElement | undefined;
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
      ref={(el) => {
        element = el;
        observer.observe(el);
      }}
      data-surface={id}
      data-size={size() ? `${size()!.w}x${size()!.h}` : undefined}
    >
      <Show when={info()} fallback={<p class="linux-waiting">The app is not connected.</p>}>
        {(surface) => (
          <SurfaceCanvas
            connection={props.context.connection}
            id={id}
            left={-origin(surface().geometry).x}
            top={-origin(surface().geometry).y}
            scale={surface().scale}
          />
        )}
      </Show>
      <For each={popups()} keyed={(popup: SurfaceInfo) => popup.id}>
        {(popup) => (
          <SurfaceCanvas
            connection={props.context.connection}
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
