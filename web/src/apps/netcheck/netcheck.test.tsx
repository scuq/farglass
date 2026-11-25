import { render } from "@solidjs/web";
import { flush } from "solid-js";
import { afterEach, describe, expect, test } from "vitest";
import type { ChannelHandlers, NetcheckEvent, NetcheckItem, NetcheckRequest, Open } from "../../proto";
import type { AppContext } from "../types";
import { age, anyOf, type CheckResult, NetcheckClient, summarize } from "./netcheck";
import NetcheckApp from "./Netcheck";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

/** A fake agent: it keeps the requests and gives events to the client. */
function fakeAgent() {
  const sent: NetcheckRequest[] = [];
  let handlers: ChannelHandlers | undefined;
  let opened: Open | undefined;
  const openChannel = (open: Open, given?: ChannelHandlers) => {
    opened = open;
    handlers = given;
    return { control: (request: NetcheckRequest) => sent.push(request), close: () => {} } as never;
  };
  return {
    sent,
    openChannel,
    opened: () => opened,
    emit: (event: NetcheckEvent) => handlers?.oncontrol?.(event as never),
    close: (reason: string) => handlers?.onclose?.({ reason } as never),
  };
}

const ITEMS: NetcheckItem[] = [
  { id: "internet:1.1.1.1", group: "internet", name: "Cloudflare", target: "https://1.1.1.1" },
  { id: "internet:google.com", group: "internet", name: "Google", target: "https://google.com" },
  { id: "dns", group: "dns", name: "Name resolution", target: "google.com" },
  { id: "admin:0", group: "admin", name: "vpn", target: "TCP vpn.example:443" },
  { id: "user:0", group: "user", name: "git", target: "TCP git.example:22" },
];

const loaded = (items = ITEMS): NetcheckEvent => ({
  type: "loaded",
  path: "/home/alice/.config/farglass/netchecks.toml",
  admin_path: "/etc/farglass/netchecks.toml",
  user: [{ name: "git", kind: "tcp", host: "git.example", port: 22 }],
  items,
  problems: [],
});

const result = (status: CheckResult["status"]): CheckResult => ({ status, detail: "", at: 0 });

describe("summary", () => {
  test("the internet is available if one check passes, and unknown while one runs", () => {
    expect(anyOf(["failed", "ok"])).toBe("ok");
    expect(anyOf(["failed", "checking"])).toBe("checking");
    expect(anyOf(["timeout", "timeout"])).toBe("timeout");
    expect(anyOf(["timeout", "failed"])).toBe("failed");
    expect(anyOf([])).toBe("not-checked");
  });

  test("the checks of the files count the passed ones and the ones with a result", () => {
    const summary = summarize(ITEMS, { "internet:google.com": result("ok"), dns: result("failed"), "admin:0": result("ok"), "user:0": result("checking") });
    expect(summary.internet).toBe("ok");
    expect(summary.dns).toBe("failed");
    expect(summary.checks).toEqual({ ok: 1, done: 1, total: 2 });
  });

  test("the age of a result has a unit", () => {
    expect(age(0, 12_000)).toBe("12 s ago");
    expect(age(0, 6 * 60_000)).toBe("6 min ago");
  });
});

describe("client", () => {
  test("the first load runs all checks, and a result replaces the running state", () => {
    const agent = fakeAgent();
    const client = new NetcheckClient(agent.openChannel, () => 1000);
    expect(agent.opened()).toEqual({ kind: "netcheck" });
    agent.emit(loaded());
    flush();
    expect(agent.sent).toEqual([{ type: "run" }]);
    expect(client.results()["dns"]?.status).toBe("checking");
    agent.emit({ type: "result", id: "dns", state: "ok", ms: 4, detail: "google.com is 192.0.2.1." });
    flush();
    expect(client.results()["dns"]).toEqual({ status: "ok", ms: 4, detail: "google.com is 192.0.2.1.", at: 1000 });
    client.run(["user:0"]);
    expect(agent.sent.at(-1)).toEqual({ type: "run", ids: ["user:0"] });
  });

  test("a save runs the checks of the user again, and a failed save rejects", async () => {
    const agent = fakeAgent();
    const client = new NetcheckClient(agent.openChannel);
    agent.emit(loaded());
    agent.sent.length = 0;
    const saved = client.save([{ name: "git", kind: "tcp", host: "git.example", port: 2222 }]);
    agent.emit(loaded());
    await saved;
    expect(agent.sent).toEqual([
      { type: "save", checks: [{ name: "git", kind: "tcp", host: "git.example", port: 2222 }] },
      { type: "run", ids: ["user:0"] },
    ]);
    const failed = client.save([]);
    agent.emit({ type: "failed", message: "The file is not valid TOML." });
    await expect(failed).rejects.toBe("The file is not valid TOML.");
  });

  test("an old agent gives a message and no running checks", () => {
    const agent = fakeAgent();
    const client = new NetcheckClient(agent.openChannel);
    agent.emit(loaded());
    agent.close("not-supported");
    flush();
    expect(client.closed()).toMatch(/older than the shell/);
    expect(Object.keys(client.results())).toEqual([]);
  });
});

describe("app", () => {
  test("the page shows a word and a mark for each state, and the DNS alert", async () => {
    const agent = fakeAgent();
    const context = { openChannel: agent.openChannel } as unknown as AppContext;
    const element = document.createElement("div");
    document.body.append(element);
    dispose = render(() => <NetcheckApp context={context} />, element);
    agent.emit(loaded());
    flush();
    agent.emit({ type: "result", id: "internet:1.1.1.1", state: "ok", ms: 76, detail: "HTTPS works: the server answered HTTP 301." });
    agent.emit({ type: "result", id: "dns", state: "timeout", detail: "No answer in 3000 ms." });
    agent.emit({ type: "result", id: "user:0", state: "failed", detail: "Refused." });
    flush();
    const row = (id: string) => element.querySelector(`[data-id="${id}"]`);
    expect(row("internet:1.1.1.1")?.textContent).toContain("Available");
    expect(row("internet:1.1.1.1")?.textContent).toContain("76 ms");
    expect(row("dns")?.textContent).toContain("No answer");
    expect(row("user:0")?.textContent).toContain("Failed");
    expect(row("admin:0")?.textContent).toContain("Checking");
    expect(element.querySelector('[role="alert"]')?.textContent).toContain("Name resolution fails");
    expect(element.querySelector('[data-field="checks-count"]')?.textContent).toContain("0 of 2 available");
    (element.querySelector('[aria-label="Check git again"]') as HTMLButtonElement).click();
    flush();
    expect(agent.sent.at(-1)).toEqual({ type: "run", ids: ["user:0"] });
  });
});
