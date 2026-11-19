import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "settings",
  title: "Settings",
  description: "Appearance, date and time, network, users, and power",
  keywords: ["preferences", "theme", "time", "timezone", "hostname", "network", "users", "sessions", "restart", "power off"],
  icon,
  size: { w: 760, h: 520 },
  start: () => import("./Settings"),
});
