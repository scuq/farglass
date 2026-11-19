import type { JSX } from "@solidjs/web";
import { createSignal, onCleanup, Show } from "solid-js";
import { DbusError } from "../../dbus";

/** Returns a sentence for the user about a failed D-Bus call. */
export function describeError(error: unknown): string {
  if (!(error instanceof DbusError)) {
    return error instanceof Error ? error.message : String(error);
  }
  switch (error.dbusName) {
    case "org.freedesktop.PolicyKit1.Error.NotAuthorized":
    case "org.freedesktop.PolicyKit1.Error.Cancelled":
    case "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired":
      return "The change was not authorized. Nothing changed.";
    case "org.freedesktop.DBus.Error.AccessDenied":
      return `Access denied: ${error.message}`;
    case "org.freedesktop.DBus.Error.ServiceUnknown":
    case "org.freedesktop.DBus.Error.NameHasNoOwner":
      return "The service is not available on this host.";
    case "org.freedesktop.DBus.Error.Disconnected":
      return "The connection to the host stopped.";
    default:
      return error.message;
  }
}

/** The result of the last action of a page, for a status line. */
export type Outcome = { readonly kind: "ok" | "error"; readonly text: string } | null;

/**
 * Runs `action` and gives its outcome to `report`: `done` after a success, or the error.
 * Returns a function for an event handler.
 */
export function runAction(action: () => Promise<unknown>, report: (outcome: Outcome) => void, done: string): Promise<void> {
  report(null);
  return action().then(
    () => report({ kind: "ok", text: done }),
    (error: unknown) => report({ kind: "error", text: describeError(error) }),
  );
}

/** The status line of a page: a success as status, an error as alert. */
export function OutcomeLine(props: { outcome: Outcome }) {
  return (
    <p class="settings-outcome" data-kind={props.outcome?.kind} role={props.outcome?.kind === "error" ? "alert" : "status"}>
      {props.outcome?.text ?? ""}
    </p>
  );
}

/** Reports whether the browser has modal dialogs. A test DOM can have none. */
const MODAL = typeof HTMLDialogElement !== "undefined" && "showModal" in HTMLDialogElement.prototype;

/**
 * A confirm step for an action that cannot be undone. Cancel has the focus first,
 * and Escape cancels. The button of the action names the action.
 */
export function ConfirmDialog(props: {
  title: string;
  children: JSX.Element;
  action: string;
  onconfirm: () => void;
  oncancel: () => void;
}) {
  let dialog: HTMLDialogElement | undefined;
  let cancel: HTMLButtonElement | undefined;
  queueMicrotask(() => {
    if (MODAL && dialog && !dialog.open) {
      dialog.showModal();
    }
    cancel?.focus();
  });
  onCleanup(() => {
    if (MODAL && dialog?.open) {
      dialog.close();
    }
  });
  return (
    <dialog
      class="settings-confirm"
      aria-labelledby="settings-confirm-title"
      ref={(el) => {
        dialog = el;
        el.addEventListener("cancel", (event) => {
          event.preventDefault();
          props.oncancel();
        });
      }}
    >
      <h2 id="settings-confirm-title">{props.title}</h2>
      <div class="settings-confirm-text">{props.children}</div>
      <div class="settings-buttons">
        <button
          type="button"
          ref={(el) => {
            cancel = el;
          }}
          onClick={() => props.oncancel()}
        >
          Cancel
        </button>
        <button type="button" class="danger" onClick={() => props.onconfirm()}>
          {props.action}
        </button>
      </div>
    </dialog>
  );
}

/** A confirm step that opens with `ask` and runs `action` after the confirm. */
export function createConfirm<T>() {
  const [pending, setPending] = createSignal<T | null>(null);
  return {
    pending,
    ask: (value: T) => setPending(() => value),
    clear: () => setPending(null),
  };
}

/** Shows "Loading" or the error until `when` has a value. `children` gets an accessor of the value, which follows each change. */
export function Loading<T>(props: { when: T | null | undefined; error: string | null; children: (value: () => T) => JSX.Element }) {
  return (
    <Show when={props.when} fallback={<p class="settings-loading" role={props.error ? "alert" : "status"}>{props.error ?? "Loading"}</p>}>
      {(value) => props.children(value)}
    </Show>
  );
}
