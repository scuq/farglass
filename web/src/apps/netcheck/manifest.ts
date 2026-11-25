import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "netcheck",
  title: "Network check",
  description: "See if the host reaches the internet, DNS, and the hosts and ports that you need",
  keywords: ["network", "connection", "internet", "dns", "ping", "icmp", "tcp", "port", "firewall", "blocked", "proxy", "diagnostics"],
  icon,
  size: { w: 640, h: 560 },
  start: () => import("./Netcheck"),
});
