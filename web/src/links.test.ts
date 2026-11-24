import { afterEach, describe, expect, test, vi } from "vitest";
import { isWebUrl, openInClient } from "./links";

afterEach(() => vi.restoreAllMocks());

describe("links", () => {
  test("only http and https URLs with a host leave the host", () => {
    expect(isWebUrl("https://example.com/a")).toBe(true);
    expect(isWebUrl("http://10.0.0.1:8080/")).toBe(true);
    for (const bad of ["javascript:alert(1)", "file:///etc/passwd", "data:text/html,x", "example.com", ""]) {
      expect(isWebUrl(bad), bad).toBe(false);
    }
  });

  test("a link opens a new tab with no handle to the shell", () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    expect(openInClient("https://example.com/")).toBe(true);
    expect(open).toHaveBeenCalledWith("https://example.com/", "_blank", "noopener,noreferrer");
    expect(openInClient("javascript:alert(1)")).toBe(false);
    expect(open).toHaveBeenCalledTimes(1);
  });
});
