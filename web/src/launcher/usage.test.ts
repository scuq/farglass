import { expect, test } from "vitest";
import { EMPTY_LAUNCHER, frequent, parseLauncherState, recordUse, togglePin } from "./usage";

const DAY = 24 * 60 * 60 * 1000;

test("a stored value that is not valid gives an empty state, and bad fields are dropped", () => {
  expect(parseLauncherState(null)).toEqual(EMPTY_LAUNCHER);
  expect(parseLauncherState({ pinned: ["a", 1, "a"], used: { a: { score: 2, last: 5 }, b: { score: "x", last: 1 }, c: null } })).toEqual({
    pinned: ["a"],
    used: { a: { score: 2, last: 5 } },
  });
});

test("the app with more starts comes first, and an old start counts less than a new one", () => {
  let state = EMPTY_LAUNCHER;
  for (let i = 0; i < 3; i++) state = recordUse(state, "old", 0);
  state = recordUse(state, "new", 60 * DAY);
  state = recordUse(state, "new", 60 * DAY);
  // 60 days later, the three old starts count less than the two new ones.
  expect(frequent(state, 60 * DAY)).toEqual(["new", "old"]);
  expect(frequent(state, 60 * DAY, 1)).toEqual(["new"]);
});

test("a pin goes to the end, a second toggle removes it, and a pinned app is not in the most used apps", () => {
  let state = recordUse(recordUse(EMPTY_LAUNCHER, "a", 0), "b", 0);
  state = togglePin(togglePin(state, "b"), "c");
  expect(state.pinned).toEqual(["b", "c"]);
  expect(frequent(state, 0)).toEqual(["a"]);
  expect(togglePin(state, "b").pinned).toEqual(["c"]);
});
