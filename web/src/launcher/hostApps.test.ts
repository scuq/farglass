import { expect, test } from "vitest";
import { APPS } from "../apps/registry";
import logo from "../brand/farglass.svg";
import { windowIcon } from "./hostApps";

test("a window of a shell app has the icon of the app", () => {
  expect(windowIcon({ id: "w1", app: "about" })).toBe(APPS.about?.icon);
});

test("a window of an unknown app has the farglass icon", () => {
  expect(windowIcon({ id: "w1", app: "no-such-app" })).toBe(logo);
});

test("a host window with no icon from the agent has the icon of the Linux app", () => {
  expect(windowIcon({ id: "w1", app: "linux" })).toBe(APPS.linux?.icon ?? logo);
});
