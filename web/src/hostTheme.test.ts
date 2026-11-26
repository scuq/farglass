import { afterEach, describe, expect, test } from "vitest";
import { applyAppearance, DEFAULT_APPEARANCE } from "./appearance";
import { startHostTheme } from "./hostTheme";
import type { Connection } from "./proto";

/** A connection that records the open message and the control messages of its one channel. */
function fakeConnection() {
  const sent: unknown[] = [];
  const opened: unknown[] = [];
  let closed = false;
  const connection = {
    openChannel(open: unknown) {
      opened.push(open);
      return {
        control: (message: unknown) => sent.push(message),
        close: () => {
          closed = true;
        },
      };
    },
  } as unknown as Connection;
  return { connection, sent, opened, closed: () => closed };
}

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("the theme of the shell for the apps of the host", () => {
  test("the shell sends its theme at the start and after each change, once for each change", () => {
    applyAppearance({ ...DEFAULT_APPEARANCE, theme: "lcars" });
    const { connection, sent, opened, closed } = fakeConnection();
    const stop = startHostTheme(connection);
    expect(opened).toEqual([{ kind: "theme" }]);
    expect(sent).toEqual([{ type: "color-scheme", dark: true }]);
    applyAppearance({ ...DEFAULT_APPEARANCE, theme: "light" });
    expect(sent).toEqual([
      { type: "color-scheme", dark: true },
      { type: "color-scheme", dark: false },
    ]);
    // A change that keeps the theme light sends nothing.
    applyAppearance({ ...DEFAULT_APPEARANCE, theme: "light", accent: "green" });
    applyAppearance({ ...DEFAULT_APPEARANCE, theme: "dark" });
    expect(sent.at(-1)).toEqual({ type: "color-scheme", dark: true });
    expect(sent).toHaveLength(3);
    stop();
    expect(closed()).toBe(true);
    applyAppearance({ ...DEFAULT_APPEARANCE, theme: "light" });
    expect(sent).toHaveLength(3);
  });
});
