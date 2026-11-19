/** The apps of the shell: the manifests from `src/apps/<id>/manifest.ts`. */

import { lazy } from "solid-js";
import type { AppComponent, AppManifest } from "./types";

/** An app: its manifest and its component, which loads on first use. */
export interface App extends AppManifest {
  /** The component of the window. Its module loads at its first render. */
  readonly component: AppComponent & { preload: () => Promise<unknown> };
}

/** Returns the apps of `modules`, by id. Throws if an id is not the name of its folder. */
export function registry(modules: Record<string, { manifest: AppManifest }>): Readonly<Record<string, App>> {
  const apps: Record<string, App> = {};
  for (const [path, module] of Object.entries(modules)) {
    const manifest = module.manifest;
    const folder = /\/([^/]+)\/manifest\.ts$/.exec(path)?.[1];
    if (manifest.id !== folder) {
      throw new Error(`The app in ${path} has the id ${manifest.id}. The id must be the name of its folder.`);
    }
    apps[manifest.id] = { ...manifest, component: lazy(manifest.start) };
  }
  return apps;
}

/** The apps, by id. */
export const APPS = registry(import.meta.glob<{ manifest: AppManifest }>("./*/manifest.ts", { eager: true }));
