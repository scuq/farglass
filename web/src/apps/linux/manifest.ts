import { defineApp } from "../types";
import icon from "./icon.svg";

/** A window of a Wayland app of the session. The surface manager opens it; the launcher does not show it. */
export const manifest = defineApp({
  id: "linux",
  title: "Linux app",
  icon,
  hidden: true,
  size: { w: 640, h: 480 },
  start: () => import("./Linux"),
});
