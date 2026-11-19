import type { JSX } from "@solidjs/web";
import { type Accessor, onCleanup } from "solid-js";
import { type Edge, type Rect, resizeRect, type SnapZone, snapZoneAt, unsnapRect } from "./geometry";
import { shownRect, type Win } from "./model";

/** The commands of a window frame to the desktop. */
export interface FrameActions {
  focus(id: string): void;
  close(id: string): void;
  minimize(id: string): void;
  toggleMaximize(id: string): void;
  /** The end of a move or a size change: a floating rectangle, or a snap zone. */
  place(id: string, rect: Rect, zone: SnapZone | null): void;
  /** The snap zone under the pointer during a move, for the preview. */
  preview(zone: SnapZone | null): void;
}

const EDGES: readonly Edge[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

/**
 * One window: a title bar, the content, and the edges for a size change.
 *
 * A move or a size change writes only the transform and the size of the
 * element, one time for each frame of the screen. The desktop state changes
 * one time, at the end of the gesture.
 */
export function WindowFrame(props: {
  icon: string | undefined;
  win: Accessor<Win>;
  area: Accessor<Rect>;
  focused: Accessor<boolean>;
  hidden: Accessor<boolean>;
  z: Accessor<number>;
  actions: FrameActions;
  children: JSX.Element;
}) {
  let element: HTMLElement | undefined;
  let frame = 0;
  let pending: Rect | null = null;

  const rect = () => shownRect(props.win(), props.area());

  /** Writes `next` to the element at the next frame. A later write in the same frame replaces it. */
  const draw = (next: Rect) => {
    pending = next;
    if (frame === 0) {
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (element && pending) {
          element.style.transform = `translate3d(${pending.x}px, ${pending.y}px, 0)`;
          element.style.width = `${pending.w}px`;
          element.style.height = `${pending.h}px`;
        }
      });
    }
  };

  onCleanup(() => cancelAnimationFrame(frame));

  /** Runs a pointer gesture with pointer capture. `move` returns the rectangle for the pointer. */
  const gesture = (
    event: PointerEvent,
    move: (dx: number, dy: number, pointer: PointerEvent) => Rect,
    end: (last: Rect, pointer: PointerEvent) => void,
  ) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture?.(event.pointerId);
    const startX = event.clientX;
    const startY = event.clientY;
    let last = rect();
    element?.classList.add("wm-dragging");

    const onMove = (pointer: PointerEvent) => {
      last = move(pointer.clientX - startX, pointer.clientY - startY, pointer);
      draw(last);
    };
    const finish = (pointer: PointerEvent, cancel: boolean) => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("keydown", onKey, true);
      element?.classList.remove("wm-dragging");
      props.actions.preview(null);
      cancelAnimationFrame(frame);
      frame = 0;
      pending = null;
      if (cancel) {
        // Draw the rectangle of the state again.
        const back = rect();
        if (element) {
          element.style.transform = `translate3d(${back.x}px, ${back.y}px, 0)`;
          element.style.width = `${back.w}px`;
          element.style.height = `${back.h}px`;
        }
      } else {
        end(last, pointer);
      }
    };
    const onUp = (pointer: PointerEvent) => finish(pointer, false);
    const onCancel = (pointer: PointerEvent) => finish(pointer, true);
    // Escape stops the gesture, and the window goes back.
    const onKey = (key: KeyboardEvent) => {
      if (key.key === "Escape") {
        key.preventDefault();
        finish(event, true);
      }
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onCancel);
    window.addEventListener("keydown", onKey, true);
  };

  const startMove = (event: PointerEvent) => {
    if ((event.target as HTMLElement).closest("button")) {
      return;
    }
    props.actions.focus(props.win().id);
    const area = props.area();
    const shown = rect();
    // A maximized or snapped window gets its floating size back under the pointer.
    const start = props.win().snap ? unsnapRect(props.win().rect, shown, event.clientX - area.x, event.clientY - area.y) : shown;
    let zone: SnapZone | null = null;
    gesture(
      event,
      (dx, dy, pointer) => {
        const next = snapZoneAt(pointer.clientX - area.x + area.x, pointer.clientY - area.y + area.y, area);
        if (next !== zone) {
          zone = next;
          props.actions.preview(zone);
        }
        return { ...start, x: start.x + dx, y: start.y + dy };
      },
      (last) => props.actions.place(props.win().id, last, zone),
    );
  };

  const startResize = (edge: Edge) => (event: PointerEvent) => {
    props.actions.focus(props.win().id);
    const start = rect();
    gesture(
      event,
      (dx, dy) => resizeRect(start, edge, dx, dy),
      (last) => props.actions.place(props.win().id, last, null),
    );
  };

  return (
    <section
      ref={(el) => {
        element = el;
      }}
      class={props.focused() ? "wm-window wm-focused" : "wm-window"}
      data-mode={props.win().snap ?? "normal"}
      data-window={props.win().id}
      aria-label={props.win().title}
      tabindex={-1}
      hidden={props.hidden()}
      style={{
        transform: `translate3d(${rect().x}px, ${rect().y}px, 0)`,
        width: `${rect().w}px`,
        height: `${rect().h}px`,
        "z-index": String(props.z()),
      }}
      onPointerDown={() => props.actions.focus(props.win().id)}
    >
      <header
        class="wm-titlebar"
        onPointerDown={startMove}
        onDblClick={(event) => {
          if (!(event.target as HTMLElement).closest("button")) {
            props.actions.toggleMaximize(props.win().id);
          }
        }}
      >
        {props.icon ? <img class="wm-icon" src={props.icon} alt="" draggable={false} /> : null}
        <span class="wm-title">{props.win().title}</span>
        <button type="button" aria-label="Minimize" title="Minimize" onClick={() => props.actions.minimize(props.win().id)}>
          ▁
        </button>
        <button
          type="button"
          aria-label={props.win().snap === "maximized" ? "Restore" : "Maximize"}
          title={props.win().snap === "maximized" ? "Restore" : "Maximize"}
          onClick={() => props.actions.toggleMaximize(props.win().id)}
        >
          {props.win().snap === "maximized" ? "❐" : "□"}
        </button>
        <button type="button" aria-label="Close" title="Close" onClick={() => props.actions.close(props.win().id)}>
          ✕
        </button>
      </header>
      <div class="wm-content">{props.children}</div>
      {props.win().snap === null
        ? EDGES.map((edge) => <div class={`wm-edge wm-edge-${edge}`} onPointerDown={startResize(edge)} />)
        : null}
    </section>
  );
}
