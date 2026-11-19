import { render } from "@solidjs/web";
import { afterEach, expect, test } from "vitest";
import { App } from "./App";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

test("the shell renders the echo panel in the connecting state", () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App />, host);
  expect(host.querySelector("main.shell h1")?.textContent).toBe("Echo channel");
  expect(host.querySelector("[role=status]")?.textContent).toBe("Connecting");
  expect(host.querySelector<HTMLInputElement>("#echo-input")?.disabled).toBe(true);
});
