import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { dataFrame, encodeJsonFrame, FrameType, HEADER_LEN, readyFrame } from "../../proto";
import { fakeConnection } from "../../proto/testing";
import { startStats } from "../../stats";
import type { AppContext } from "../types";
import { ResourcesPage, scoreOf } from "./pages/Resources";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("resource score", () => {
  test("the score has three levels from the CPU of farglass and the data rate", () => {
    expect(scoreOf(3, 50_000)).toBe("light");
    expect(scoreOf(30, 50_000)).toBe("moderate");
    expect(scoreOf(3, 1_000_000)).toBe("moderate");
    expect(scoreOf(80, 0)).toBe("heavy");
    expect(scoreOf(3, 5_000_000)).toBe("heavy");
  });

  test("the page shows farglassd, the agent, the data rate of the tab, and the score", async () => {
    const { server, connection } = await fakeConnection("alice");
    const stop = startStats(connection);
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const host = document.createElement("div");
    document.body.append(host);
    dispose = render(() => <ResourcesPage context={{ connection } as unknown as AppContext} />, host);
    server.take();
    server.push(readyFrame(1));
    server.push(
      encodeJsonFrame(1, FrameType.Control, {
        type: "sample",
        windows: [],
        agent: { cpu: 2.5, memory: 48_000_000 },
        server: { cpu: 1.25, memory: 30_000_000 },
      }),
    );
    // The shell sends 400 kB in 2 s: 200 kB/s out.
    for (let i = 0; i < 4; i++) connection.sendRaw(dataFrame(1, new Uint8Array(100_000 - HEADER_LEN)));
    await vi.advanceTimersByTimeAsync(2000);
    expect(host.querySelector('[data-field="server"]')?.textContent).toBe("CPU 1.3 %, memory 30 MB");
    expect(host.querySelector('[data-field="agent"]')?.textContent).toBe("CPU 2.5 %, memory 48 MB");
    expect(host.querySelector('[data-field="rate"]')?.textContent).toMatch(/^\d+ B\/s in, 200 kB\/s out$/);
    expect(host.querySelector("[data-score]")?.getAttribute("data-score")).toBe("moderate");
    stop();
  });
});
