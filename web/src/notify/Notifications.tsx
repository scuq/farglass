import { createSignal, For, onCleanup, Show } from "solid-js";
import type { Connection, Notification, NotifyEvent, NotifyReply } from "../proto";
import { HOLD_MAX_MS } from "../wm/focus";
import { Timers } from "./timers";

/** What the notifications need for focus protection (F041). */
export interface NotificationHold {
  /** Reports whether notifications wait while the user is busy. */
  enabled(): boolean;
  /** Reports whether the user types or moves the pointer now. */
  busy(): boolean;
  /** Calls `handler` at the start of each pause. Returns the function that stops the calls. */
  onPause(handler: () => void): () => void;
}
import "./notify.css";

/** The largest number of notifications on the screen. The others wait for a free place. */
export const MAX_SHOWN = 4;

/** The action that a click on the notification itself invokes. */
const DEFAULT_ACTION = "default";

/**
 * The desktop notifications of the session, at the top right. The component
 * keeps a notify channel open. The agent sends each open notification and each
 * change. The time to show starts when the notification is on the screen.
 *
 * With `hold`, a new notification waits while the user types or moves the pointer,
 * and shows at the next pause, or after {@link HOLD_MAX_MS}. A critical one shows at once.
 */
export function Notifications(props: { connection: Connection; hold?: NotificationHold }) {
  // The logic uses a plain list: Solid commits a signal write on the next microtask,
  // and several events can come in the same task. The signal is a copy for the view.
  let items: Notification[] = [];
  const [view, setView] = createSignal<readonly Notification[]>([]);
  let section: HTMLElement | undefined;

  const send = (reply: NotifyReply) => channel.control(reply);
  const timers = new Timers((id) => {
    send({ type: "expire", id });
    remove(id);
  });

  // Each notification on the screen gets its time one time. A waiting one gets it when it comes on the screen.
  const started = new Set<number>();
  const commit = () => {
    setView(items);
    // A removed notification can take the focus or the pointer with it, with no focusout and no mouseleave.
    queueMicrotask(pauseOrResume);
    for (const notification of items.slice(0, MAX_SHOWN)) {
      if (!started.has(notification.id)) {
        started.add(notification.id);
        timers.set(notification.id, notification.timeout_ms ?? null);
      }
    }
  };

  const remove = (id: number) => {
    timers.remove(id);
    started.delete(id);
    items = items.filter((item) => item.id !== id);
    commit();
  };

  // The notifications that wait for a pause of the user, in their order.
  let held: Notification[] = [];
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  const release = () => {
    clearTimeout(holdTimer);
    holdTimer = undefined;
    if (held.length === 0) return;
    items = [...items, ...held];
    held = [];
    commit();
  };
  if (props.hold) onCleanup(props.hold.onPause(release));
  onCleanup(() => clearTimeout(holdTimer));

  const onEvent = (event: NotifyEvent) => {
    switch (event.type) {
      case "show": {
        const { notification } = event;
        const waiting = held.findIndex((item) => item.id === notification.id);
        if (waiting >= 0) {
          held = held.with(waiting, notification);
          if (notification.urgency === "critical") release();
          break;
        }
        const index = items.findIndex((item) => item.id === notification.id);
        if (index < 0 && notification.urgency !== "critical" && props.hold?.enabled() && props.hold.busy()) {
          held = [...held, notification];
          holdTimer ??= setTimeout(release, HOLD_MAX_MS);
          break;
        }
        if (index >= 0) {
          // A replacement: the same place, and the time starts again.
          items = items.with(index, notification);
          timers.remove(notification.id);
          started.delete(notification.id);
        } else {
          items = [...items, notification];
        }
        commit();
        break;
      }
      case "close":
        held = held.filter((item) => item.id !== event.id);
        remove(event.id);
        break;
    }
  };

  const channel = props.connection.openChannel({ kind: "notify" }, { oncontrol: (message) => onEvent(message as NotifyEvent) });

  // The user looks at the notifications: the pointer is on them, one has the focus, or the tab is hidden.
  const pauseOrResume = () => {
    if (section?.matches(":hover, :focus-within") || document.hidden) {
      timers.pause();
    } else {
      timers.resume();
    }
  };
  document.addEventListener("visibilitychange", pauseOrResume);
  pauseOrResume();
  onCleanup(() => {
    document.removeEventListener("visibilitychange", pauseOrResume);
    timers.clear();
    channel.close();
  });

  const dismiss = (id: number) => {
    send({ type: "dismiss", id });
    remove(id);
  };
  const invoke = (notification: Notification, key: string) => {
    send({ type: "action", id: notification.id, key });
    // The agent closes the notification after the action, unless the program keeps it.
  };

  const shown = () => view().slice(0, MAX_SHOWN);
  const waiting = () => Math.max(0, view().length - MAX_SHOWN);

  return (
    <section
      class="notifications"
      aria-label="Notifications"
      ref={(el) => {
        section = el;
      }}
      onMouseEnter={() => pauseOrResume()}
      onMouseLeave={() => queueMicrotask(pauseOrResume)}
      onFocusIn={() => pauseOrResume()}
      onFocusOut={() => queueMicrotask(pauseOrResume)}
    >
      <For each={shown()} keyed={(notification) => notification.id}>
        {(notification) => {
          const hasDefault = () => notification().actions.some((action) => action.key === DEFAULT_ACTION);
          const buttons = () => notification().actions.filter((action) => action.key !== DEFAULT_ACTION);
          return (
            <article
              class="notification"
              data-urgency={notification().urgency}
              role={notification().urgency === "critical" ? "alert" : "status"}
              aria-label={notification().summary}
            >
              <header>
                <span class="notification-app">{notification().app}</span>
                <button type="button" class="notification-close" aria-label="Dismiss" title="Dismiss" onClick={() => dismiss(notification().id)}>
                  ×
                </button>
              </header>
              <Show
                when={hasDefault()}
                fallback={
                  <div class="notification-text">
                    <strong>{notification().summary}</strong>
                    <Show when={notification().body}>
                      <p>{notification().body}</p>
                    </Show>
                  </div>
                }
              >
                <button type="button" class="notification-text" onClick={() => invoke(notification(), DEFAULT_ACTION)}>
                  <strong>{notification().summary}</strong>
                  <Show when={notification().body}>
                    <p>{notification().body}</p>
                  </Show>
                </button>
              </Show>
              <Show when={buttons().length > 0}>
                <div class="notification-actions">
                  <For each={buttons()}>
                    {(action) => (
                      <button type="button" onClick={() => invoke(notification(), action.key)}>
                        {action.label}
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </article>
          );
        }}
      </For>
      <Show when={waiting() > 0}>
        <p class="notification-more">{waiting()} more</p>
      </Show>
    </section>
  );
}
