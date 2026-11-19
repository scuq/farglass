import { afterEach, expect, test } from "vitest";
import { render } from "@solidjs/web";
import { App } from "./App";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

test("the shell renders its root element", () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App />, host);
  expect(host.querySelector("main.shell")?.textContent).toBe("farglass");
});
