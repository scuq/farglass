import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "about",
  title: "About farglass",
  description: "The version and the connection",
  keywords: ["version", "info", "help"],
  icon,
  size: { w: 420, h: 300 },
  start: () => import("./About"),
});
