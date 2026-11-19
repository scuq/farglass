// @vitest-environment node
import { afterEach, describe, expect, test, vi } from "vitest";
import { ACK_DELAY_MS, ACK_STEP, AckCounter, ECHO_MEASURE, EchoProbe } from "./flow";

afterEach(() => {
  vi.useRealTimers();
  performance.clearMeasures();
});

describe("AckCounter", () => {
  test("a full step goes at once", () => {
    const sent: number[] = [];
    const acks = new AckCounter((bytes) => sent.push(bytes));
    acks.parsed(ACK_STEP - 1);
    expect(sent).toEqual([]);
    acks.parsed(10);
    expect(sent).toEqual([ACK_STEP + 9]);
  });

  test("small output goes after a short time as one acknowledgment", () => {
    vi.useFakeTimers();
    const sent: number[] = [];
    const acks = new AckCounter((bytes) => sent.push(bytes));
    acks.parsed(5);
    acks.parsed(7);
    expect(sent).toEqual([]);
    vi.advanceTimersByTime(ACK_DELAY_MS);
    expect(sent).toEqual([12]);
    vi.advanceTimersByTime(1000);
    expect(sent).toEqual([12]);
  });

  test("the window never fills: the acknowledged bytes equal the parsed bytes", () => {
    vi.useFakeTimers();
    let acknowledged = 0;
    const acks = new AckCounter((bytes) => (acknowledged += bytes));
    let parsed = 0;
    for (let i = 0; i < 1000; i++) {
      const chunk = (i * 7919) % 40_000;
      parsed += chunk;
      acks.parsed(chunk);
    }
    vi.advanceTimersByTime(ACK_DELAY_MS);
    expect(acknowledged).toBe(parsed);
  });
});

describe("EchoProbe", () => {
  test("one measurement for each input, from the first input to the next output", () => {
    const probe = new EchoProbe();
    expect(probe.output(5)).toBeNull();
    probe.input(10);
    probe.input(11);
    expect(probe.output(13)).toBe(3);
    expect(probe.output(20)).toBeNull();
    const measures = performance.getEntriesByName(ECHO_MEASURE);
    expect(measures).toHaveLength(1);
    expect(measures[0]!.duration).toBe(3);
  });
});
