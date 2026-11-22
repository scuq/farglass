import { describe, expect, it } from "vitest";

import { fit, sameSize } from "./fit";

const size = (width: number, height: number) => ({ width, height });

describe("fit", () => {
  it("keeps the frame when the app has its size", () => {
    expect(fit(size(800, 600), size(800, 600), null, 0, 2000)).toBe("keep");
    expect(fit(size(801, 599), size(800, 600), null, 0, 2000)).toBe("keep");
  });

  it("takes the size of an app that answered with a different size", () => {
    // A terminal snaps 500 x 400 down to its text cells.
    expect(fit(size(496, 392), size(500, 400), null, 0, 2000)).toBe("fit");
  });

  it("takes the size of an app that changed its size itself", () => {
    expect(fit(size(640, 480), size(800, 600), null, 0, 2000)).toBe("fit");
  });

  it("waits for the answer to a size that the shell asked for", () => {
    // The first frame of a new app comes before its answer to the size of the frame.
    expect(fit(size(484, 316), size(800, 600), size(800, 600), 300, 2000)).toBe("wait");
  });

  it("does not wait for an answer after the longest wait", () => {
    expect(fit(size(484, 316), size(800, 600), size(800, 600), 2000, 2000)).toBe("fit");
  });

  it("does not wait when the app answered the request", () => {
    expect(fit(size(800, 600), size(800, 600), size(800, 600), 10, 2000)).toBe("keep");
  });
});

describe("sameSize", () => {
  it("allows a difference of 1 pixel", () => {
    expect(sameSize(size(10, 10), size(11, 9))).toBe(true);
    expect(sameSize(size(10, 10), size(12, 10))).toBe(false);
  });
});
