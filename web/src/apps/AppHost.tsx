import { Errored, Loading, onCleanup } from "solid-js";
import type { Channel } from "../proto";
import type { DesktopController } from "../wm/controller";
import type { App } from "./registry";
import type { AppContext } from "./types";

/** Reports whether an error is the failure to load a module, for example after an update of the shell. */
function isLoadError(error: unknown): boolean {
  const text = error instanceof Error ? `${error.name} ${error.message}` : String(error);
  return /dynamically imported module|Importing a module script failed|error loading dynamically|Failed to fetch/i.test(text);
}

/**
 * Runs the app of one window: it creates the context, loads the app module on
 * first use, and shows its component. Closing the window closes the channels
 * of the app.
 */
export function AppHost(props: {
  controller: Pick<DesktopController, "connection" | "state" | "trackKey" | "setTitle" | "openApp">;
  app: App | undefined;
  windowId: string;
  appId: string;
}) {
  const app = props.app;
  if (!app) {
    return <p class="app-message">The app {props.appId} is not available on this host.</p>;
  }

  const c = props.controller;
  const channels = new Set<Channel>();
  const unwatch: (() => void)[] = [];
  const context: AppContext = {
    windowId: props.windowId,
    user: c.connection.user,
    openChannel(open, handlers = {}) {
      const channel = c.connection.openChannel(open, {
        ...handlers,
        onclose: (close) => {
          channels.delete(channel);
          handlers.onclose?.(close);
        },
      });
      channels.add(channel);
      return channel;
    },
    state: {
      watch(key, onvalue) {
        c.trackKey(props.windowId, key);
        unwatch.push(c.state.watch(key, onvalue));
      },
      set(key, value) {
        c.trackKey(props.windowId, key);
        c.state.set(key, value);
      },
      key: (name) => `${props.appId}.${name}.${props.windowId}`,
    },
    setTitle: (title) => c.setTitle(props.windowId, title),
    openApp: (appId) => c.openApp(appId),
  };

  onCleanup(() => {
    for (const stop of unwatch) {
      stop();
    }
    for (const channel of channels) {
      channel.close({ reason: "cancelled" });
    }
  });

  const Component = app.component;
  return (
    <Errored
      fallback={(error, reset) =>
        isLoadError(error()) ? (
          <div class="app-message" role="alert">
            <p>The app {app.title} did not load. The shell on the host is possibly newer than this page.</p>
            <button type="button" onClick={() => window.location.reload()}>
              Reload the page
            </button>
          </div>
        ) : (
          <div class="app-message" role="alert">
            <p>
              The app {app.title} stopped with an error: {String(error())}
            </p>
            <button type="button" onClick={() => reset()}>
              Start the app again
            </button>
          </div>
        )
      }
    >
      <Loading fallback={<p class="app-message" role="status">Loading {app.title}</p>}>
        <Component context={context} />
      </Loading>
    </Errored>
  );
}
