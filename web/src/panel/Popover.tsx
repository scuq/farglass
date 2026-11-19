import type { JSX } from "@solidjs/web";
import { createEffect } from "solid-js";

/** Reports whether the browser has the Popover API. */
const SUPPORTED = typeof HTMLElement !== "undefined" && "showPopover" in HTMLElement.prototype;

/**
 * A menu in the top layer, with the Popover API: light dismiss and Escape come
 * from the browser. CSS anchor positioning puts it at its button.
 *
 * `open` and `onClose` keep the state of the caller and the browser together:
 * the browser closes the popover itself on a click outside or on Escape.
 * In a DOM with no Popover API, for example a test DOM, the content shows
 * only while `open` is true.
 */
export function Popover(props: {
  id: string;
  class: string;
  open: boolean;
  onClose: () => void;
  label: string;
  children: JSX.Element;
}) {
  let element: HTMLElement | undefined;

  createEffect(
    () => props.open,
    (open) => {
      if (!SUPPORTED || !element) {
        return;
      }
      const shown = element.matches(":popover-open");
      if (open && !shown) {
        element.showPopover();
      } else if (!open && shown) {
        element.hidePopover();
      }
    },
  );

  return (
    <div
      id={props.id}
      class={`popover ${props.class}`}
      popover="auto"
      role="dialog"
      aria-label={props.label}
      hidden={!SUPPORTED && !props.open}
      ref={(el) => {
        element = el;
        el.addEventListener("toggle", (event) => {
          if ((event as ToggleEvent).newState === "closed") {
            props.onClose();
          }
        });
      }}
    >
      {props.open ? props.children : null}
    </div>
  );
}
