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

  const sampleFrame = (extra: Record<string, unknown> = {}) =>
    encodeJsonFrame(1, FrameType.Control, {
      type: "sample",
      windows: [
        { window: "t1", usage: { cpu: 0.5, memory: 8_000_000 } },
        { window: "surface-3", usage: { cpu: 45, memory: 900_000_000, net: 2_500_000 } },
        { window: "gone", usage: { cpu: 99, memory: 1 } },
      ],
      agent: { cpu: 2.5, memory: 48_000_000 },
      server: { cpu: 1.25, memory: 30_000_000 },
      ...extra,
    });

  const mount = async () => {
    const { server, connection } = await fakeConnection("alice");
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const stop = startStats(connection);
    const host = document.createElement("div");
    document.body.append(host);
    const activated: string[] = [];
    const windows = () => [
      { id: "surface-3", title: "YouTube - Brave", app: "linux" },
      { id: "t1", title: "scuq@host: ~", app: "terminal" },
      { id: "e1", title: "Echo", app: "echo" },
    ];
    dispose = render(() => <ResourcesPage context={{ connection, windows, activateWindow: (id: string) => activated.push(id) } as unknown as AppContext} />, host);
    server.take();
    server.push(readyFrame(1));
    return { server, connection, host, activated, stop };
  };

  test("the page shows farglassd, the agent, the data rate of the tab, and the score", async () => {
    const { server, connection, host, activated, stop } = await mount();
    // The shell sends 400 kB in the 2 s before the sample: 200 kB/s out.
    for (let i = 0; i < 4; i++) connection.sendRaw(dataFrame(1, new Uint8Array(100_000 - HEADER_LEN)));
    await vi.advanceTimersByTimeAsync(2000);
    server.push(sampleFrame());
    await vi.advanceTimersByTimeAsync(0);
    expect(host.querySelector('[data-field="server"]')?.textContent).toBe("CPU 1.3 %, memory 30 MB");
    expect(host.querySelector('[data-field="agent"]')?.textContent).toBe("CPU 2.5 %, memory 48 MB");
    expect(host.querySelector('[data-field="rate"]')?.textContent).toMatch(/^\d+ B\/s in, 200 kB\/s out$/);
    expect(host.querySelector("[data-score]")?.getAttribute("data-score")).toBe("moderate");
    // An agent that does not send the use of the host: the page has no card for the host.
    expect(host.querySelector('[data-section="resources-host"]')).toBeNull();
    // Each open app with its use, the most CPU first; a row goes to its window.
    const rows = [...host.querySelectorAll(".settings-apps tbody tr")].map((row) => [...row.querySelectorAll("td")].map((cell) => cell.textContent));
    expect(rows).toEqual([
      ["YouTube - Brave", "45.0 %", "900 MB", "2.5 MB/s"],
      ["scuq@host: ~", "0.5 %", "8 MB", "–"],
    ]);
    expect(host.querySelector('[data-window="surface-3"]')?.getAttribute("data-busy")).toBe("true");
    host.querySelector<HTMLButtonElement>('[data-window="t1"] button')!.click();
    expect(activated).toEqual(["t1"]);
    stop();
  });

  test("the page shows the host with a graph of its history, and tells when the numbers are old", async () => {
    const { server, host, stop } = await mount();
    const hostUse = (cpu: number) => ({ host: { cpu, memory: 4_000_000_000, memory_total: 16_000_000_000, net_in: 1_200_000, net_out: 30_000 } });
    await vi.advanceTimersByTimeAsync(2000);
    server.push(sampleFrame(hostUse(10)));
    await vi.advanceTimersByTimeAsync(0);
    expect(host.querySelector('[data-field="host-cpu"]')?.textContent).toBe("10.0 %");
    expect(host.querySelector('[data-field="host-memory"]')?.textContent).toBe("4 GB of 16 GB");
    expect(host.querySelector('[data-field="host-net"]')?.textContent).toBe("1.2 MB/s in, 30 kB/s out");
    const cpu = () => host.querySelector('[data-section="resources-host"] .spark-plot')!;
    // One sample is no line yet.
    expect(cpu().querySelector(".spark-line")?.getAttribute("d")).toBe("");

    await vi.advanceTimersByTimeAsync(2000);
    server.push(sampleFrame(hostUse(50)));
    await vi.advanceTimersByTimeAsync(0);
    expect(cpu().querySelector(".spark-line")?.getAttribute("d")).toMatch(/^M[\d.]+ [\d.]+L300 22$/);
    expect(cpu().getAttribute("aria-label")).toBe("CPU of the host, last 5 minutes. Now 50.0 %. Peak 50.0 %.");
    expect(host.querySelector('[data-section="resources-host"] .spark-caption')?.textContent).toBe("2 s, peak 50.0 %");
    // The graph of the network has two lines and a key.
    const net = host.querySelectorAll('[data-section="resources-host"] .spark')[2]!;
    expect(net.querySelectorAll(".spark-line")).toHaveLength(2);
    expect([...net.querySelectorAll(".spark-key")].map((key) => key.textContent)).toEqual(["in", "out"]);
    expect(host.querySelector(".settings-stale")).toBeNull();

    // The agent sends no sample for 8 s.
    await vi.advanceTimersByTimeAsync(8000);
    expect(host.querySelector(".settings-stale")?.textContent).toMatch(/The numbers are old: the agent sent none for 8 s\./);
    expect(host.querySelector(".settings-page")?.getAttribute("data-stale")).toBe("true");
    stop();
  });
});
