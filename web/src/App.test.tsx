import { render } from "@solidjs/web";
import { afterEach, expect, test } from "vitest";
import { App } from "./App";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

test("the shell shows that it connects to the host", () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App connect={() => new Promise(() => {})} />, host);
  expect(host.querySelector("[role=status]")?.textContent).toBe("Connecting to the host");
});
