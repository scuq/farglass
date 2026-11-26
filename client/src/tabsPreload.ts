/**
 * The preload of the tab strip (F122). Only the tab strip has it.
 */
import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("farglassTabs", {
  select: (id: string) => ipcRenderer.invoke("tabs:select", id),
  close: (id: string) => ipcRenderer.invoke("tabs:close", id),
  onState: (listener: (state: unknown) => void) => {
    ipcRenderer.on("tabs:state", (_event, state: unknown) => listener(state));
  },
});
