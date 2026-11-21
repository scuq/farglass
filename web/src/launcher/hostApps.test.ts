import { expect, test } from "vitest";
import { APPS } from "../apps/registry";
import logo from "../brand/farglass.svg";
import calcIcon from "./calc.svg";
import { isShellIcon, windowIcon } from "./hostApps";

test("a window of a shell app has the icon of the app", () => {
  expect(windowIcon({ id: "w1", app: "about" })).toBe(APPS.about?.icon);
});

test("a window of an unknown app has the farglass icon", () => {
  expect(windowIcon({ id: "w1", app: "no-such-app" })).toBe(logo);
});

test("a host window with no icon from the agent has the icon of the Linux app", () => {
  expect(windowIcon({ id: "w1", app: "linux" })).toBe(APPS.linux?.icon ?? logo);
});

test("the icons of the shell apps and the calculator are shell icons; the farglass icon and a host icon are not", () => {
  expect(isShellIcon(APPS.about?.icon)).toBe(true);
  expect(isShellIcon(calcIcon)).toBe(true);
  expect(isShellIcon(logo)).toBe(false);
  expect(isShellIcon("blob:https://host/icon-of-a-host-app")).toBe(false);
  expect(isShellIcon(undefined)).toBe(false);
});
