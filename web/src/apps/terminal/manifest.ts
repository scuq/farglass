import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "terminal",
  title: "Terminal",
  description: "A shell on the host",
  keywords: ["shell", "console", "bash", "command line", "tty"],
  icon,
  size: { w: 760, h: 480 },
  start: () => import("./Terminal"),
});
