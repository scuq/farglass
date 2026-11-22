import { describe, expect, test } from "vitest";
import pkg from "../../package.json" with { type: "json" };
import { VERSION_CODE, versionCode } from "./version";

describe("versionCode", () => {
  test("gives two digits for each part of the version", () => {
    expect(versionCode("0.1.4")).toBe("FG-000104");
    expect(versionCode("1.12.0")).toBe("FG-011200");
    expect(versionCode("2.0.13-rc1")).toBe("FG-020013");
    expect(versionCode("3")).toBe("FG-030000");
  });

  test("the build uses the version of package.json", () => {
    expect(VERSION_CODE).toBe(versionCode(pkg.version));
  });
});
