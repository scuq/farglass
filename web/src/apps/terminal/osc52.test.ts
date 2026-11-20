import { describe, expect, test } from "vitest";
import { COPY_WINDOW_MS, CopyGate } from "./osc52";

describe("OSC 52 gate (F055)", () => {
  test("a program can write the clipboard only shortly after a key or a click", () => {
    const gate = new CopyGate();
    expect(gate.allows(1000)).toBe(false);
    gate.input(1000);
    expect(gate.allows(1000 + COPY_WINDOW_MS)).toBe(true);
    expect(gate.allows(1001 + COPY_WINDOW_MS)).toBe(false);
  });
});
