import { defineApp } from "../types";
import icon from "./icon.svg";

export const manifest = defineApp({
  id: "bookmarks",
  title: "Bookmarks",
  description: "Open URLs in a browser, a profile, or a private window",
  keywords: ["links", "urls", "browser", "profile", "private"],
  icon,
  size: { w: 560, h: 460 },
  start: () => import("./Bookmarks"),
});
