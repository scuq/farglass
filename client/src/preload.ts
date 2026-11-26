/**
 * The preload of the server list page (F121). Only the picker page has it.
 * The windows of the shell have no preload.
 */
import { contextBridge, ipcRenderer } from "electron";

type Reply<T> = { ok: true; value: T } | { ok: false; error: string };

async function call<T>(channel: string, ...args: unknown[]): Promise<T> {
  const reply = (await ipcRenderer.invoke(channel, ...args)) as Reply<T>;
  if (!reply.ok) throw new Error(reply.error);
  return reply.value;
}

contextBridge.exposeInMainWorld("farglass", {
  list: () => call("servers:list"),
  add: (input: unknown) => call("servers:add", input),
  update: (id: string, input: unknown) => call("servers:update", id, input),
  remove: (id: string) => call("servers:remove", id),
  forgetPin: (id: string) => call("servers:forget-pin", id),
  connect: (id: string) => call("servers:connect", id),
  version: () => call("app:version"),
  onChanged: (listener: (message: string | null) => void) => {
    ipcRenderer.on("servers:changed", (_event, message: string | null) => listener(message));
  },
});
