import { createSignal, For, onCleanup, Show } from "solid-js";
import type { Connection, Notification, NotifyEvent, NotifyReply } from "../proto";
import { Timers } from "./timers";
import "./notify.css";

/** The largest number of notifications on the screen. The others wait for a free place. */
export const MAX_SHOWN = 4;

/** The action that a click on the notification itself invokes. */
const DEFAULT_ACTION = "default";

/**
 * The desktop notifications of the session, at the top right. The component
 * keeps a notify channel open. The agent sends each open notification and each
 * change. The time to show starts when the notification is on the screen.
 */
export function Notifications(props: { connection: Connection }) {
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

  const onEvent = (event: NotifyEvent) => {
    switch (event.type) {
      case "show": {
        const { notification } = event;
        const index = items.findIndex((item) => item.id === notification.id);
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
