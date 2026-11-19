import { expect, test } from "vitest";
import { ACTIVITY_INTERVAL_MS, trackActivity } from "./activity";

test("the first input reports at once, then at most one report for each interval", () => {
  const target = new EventTarget();
  let time = 0;
  let reports = 0;
  const stop = trackActivity(() => (reports += 1), target, () => time);
  target.dispatchEvent(new Event("keydown"));
  target.dispatchEvent(new Event("pointermove"));
  expect(reports).toBe(1);
  time = ACTIVITY_INTERVAL_MS - 1;
  target.dispatchEvent(new Event("wheel"));
  expect(reports).toBe(1);
  time = ACTIVITY_INTERVAL_MS;
  target.dispatchEvent(new Event("pointerdown"));
  expect(reports).toBe(2);
  stop();
  time = 10 * ACTIVITY_INTERVAL_MS;
  target.dispatchEvent(new Event("keydown"));
  expect(reports).toBe(2);
});
