import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { type Close, FrameType, parseJson } from "../proto";
import { fakeConnection } from "../proto/testing";
import { StateClient } from "../state";
import { AppHost } from "./AppHost";
import { APPS, registry } from "./registry";
import type { AppComponent, AppContext, AppManifest } from "./types";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function manifest(id: string, start: AppManifest["start"]): AppManifest {
  return { id, title: `App ${id}`, icon: "icon.svg", size: { w: 300, h: 200 }, start };
}

/** Renders `app` in an AppHost with a fake controller, and returns what the controller records. */
async function host(component: () => Promise<{ default: AppComponent }>) {
  const { server, connection } = await fakeConnection("alice");
  const app = registry({ "./test/manifest.ts": { manifest: manifest("test", component) } }).test;
  const titles: string[] = [];
  const keys: string[] = [];
  const opened: string[] = [];
  const controller = {
    connection,
    state: new StateClient(connection),
    trackKey: (_id: string, key: string) => keys.push(key),
    setTitle: (_id: string, title: string) => titles.push(title),
    openApp: (id: string) => opened.push(id),
    addHook: () => () => {},
    focusedId: () => "w1",
  };
  const element = document.createElement("div");
  document.body.append(element);
  dispose = render(() => <AppHost controller={controller} app={app} windowId="w1" appId="test" />, element);
  return { element, server, titles, keys, opened };
}

describe("registry", () => {
  test("the shell finds each app by its manifest", () => {
    expect(Object.keys(APPS).sort()).toEqual(["about", "echo", "terminal"]);
    for (const app of Object.values(APPS)) {
      expect(app.icon).toBeTruthy();
      expect(typeof app.component.preload).toBe("function");
    }
  });

  test("an id that is not the name of the folder is refused", () => {
    const start = () => Promise.resolve({ default: () => null });
    expect(() => registry({ "./echo/manifest.ts": { manifest: manifest("other", start) } })).toThrow("must be the name of its folder");
  });

  test("the module of an app does not load before its first use", () => {
    let loads = 0;
    registry({ "./lazy/manifest.ts": { manifest: manifest("lazy", () => (loads++, Promise.resolve({ default: () => null }))) } });
    expect(loads).toBe(0);
  });
});

describe("AppHost", () => {
  test("the window shows Loading until the module arrives, then the app", async () => {
    let resolve!: (module: { default: AppComponent }) => void;
    const { element } = await host(() => new Promise((r) => (resolve = r)));
    await settle();
    expect(element.querySelector("[role=status]")?.textContent).toBe("Loading App test");
    resolve({ default: () => <p class="ready">ready</p> });
    await vi.waitFor(() => expect(element.querySelector(".ready")?.textContent).toBe("ready"));
  });

  test("a module that does not load gives a reload button", async () => {
    const { element } = await host(() => Promise.reject(new TypeError("Failed to fetch dynamically imported module: /assets/Echo-old.js")));
    await vi.waitFor(() => expect(element.querySelector("[role=alert]")?.textContent).toContain("did not load"));
    expect(element.querySelector("button")?.textContent).toBe("Reload the page");
  });

  test("the context gives the window id, the user, a title, other apps, and window keys", async () => {
    let context!: AppContext;
    const { titles, keys, opened } = await host(() =>
      Promise.resolve({
        default: (props) => {
          context = props.context;
          return <p>app</p>;
        },
      }),
    );
    await vi.waitFor(() => expect(context).toBeDefined());
    expect(context.windowId).toBe("w1");
    expect(context.user).toBe("alice");
    expect(context.state.key("log")).toBe("test.log.w1");
    context.setTitle("New title");
    context.openApp("echo");
    context.state.set(context.state.key("log"), [1]);
    expect(titles).toEqual(["New title"]);
    expect(opened).toEqual(["echo"]);
    expect(keys).toEqual(["test.log.w1"]);
  });

  test("closing the window closes the channels of the app", async () => {
    let context!: AppContext;
    const { server } = await host(() =>
      Promise.resolve({
        default: (props) => {
          context = props.context;
          return <p>app</p>;
        },
      }),
    );
    await vi.waitFor(() => expect(context).toBeDefined());
    const channel = context.openChannel({ kind: "echo" });
    server.take();
    dispose?.();
    dispose = undefined;
    const close = server.take().find((frame) => frame.channel === channel.id && frame.type === FrameType.Close);
    expect(parseJson<Close>(close!).reason).toBe("cancelled");
  });
});
