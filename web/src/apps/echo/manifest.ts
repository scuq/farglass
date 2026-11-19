import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "echo",
  title: "Echo",
  description: "Send a message to the agent and back",
  keywords: ["test", "channel", "ping"],
  icon,
  size: { w: 520, h: 380 },
  start: () => import("./Echo"),
});
