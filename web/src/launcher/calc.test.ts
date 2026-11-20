import { describe, expect, test } from "vitest";
import { calculate, convert, evaluate, formatNumber } from "./calc";

describe("calculator", () => {
  test("operators, powers, parentheses, functions, and constants", () => {
    expect(evaluate("1 + 2 * 3")?.value).toBe(7);
    expect(evaluate("(1 + 2) * 3")?.value).toBe(9);
    expect(evaluate("2 ^ 3 ^ 2")?.value).toBe(512);
    expect(evaluate("-2 ^ 2")?.value).toBe(-4);
    expect(evaluate("2^-1")?.value).toBe(0.5);
    expect(evaluate("10 % 4")?.value).toBe(2);
    expect(evaluate("sqrt(16) + abs(-2)")?.value).toBe(6);
    expect(evaluate("6 × 7 ÷ 2")?.value).toBe(21);
    expect(evaluate("1_000 * 2")?.value).toBe(2000);
    expect(formatNumber(evaluate("sin(pi / 2)")!.value)).toBe("1");
    expect(formatNumber(evaluate("0.1 + 0.2")!.value)).toBe("0.3");
  });

  test("a text that is not an expression gives no result, and a plain number is no calculation", () => {
    expect(evaluate("echo")).toBeNull();
    expect(evaluate("2 +")).toBeNull();
    expect(evaluate("alert(1)")).toBeNull();
    expect(evaluate("1 / 0")).toBeNull();
    expect(calculate("42")).toBeNull();
    expect(calculate("pi")).toBeNull();
    expect(calculate("e")).toBeNull();
    expect(calculate("terminal")).toBeNull();
    expect(calculate("6*7")).toEqual({ text: "6*7 = 42", value: "42" });
  });

  test("unit conversions of each dimension, with in, to, as, and =", () => {
    expect(convert("10 km in mi")?.value).toBe("6.21371192237");
    expect(convert("5 ft in in")?.value).toBe("60");
    expect(convert("100 f to c")?.text).toBe("100 °F = 37.7777777778 °C");
    expect(convert("0 °C in K")?.value).toBe("273.15");
    expect(convert("2 GiB in MB")?.value).toBe("2147.483648");
    expect(convert("1 gbit = MB")?.value).toBe("125");
    expect(convert("60 mph as km/h")?.value).toBe("96.56064");
    expect(convert("2*3 h in min")?.value).toBe("360");
    expect(convert("1 gal in l")?.value).toBe("3.785411784");
    expect(convert("1 ha in m2")?.value).toBe("10000");
    expect(convert("180 deg in rad")?.value).toBe(formatNumber(Math.PI));
    expect(convert("10 kg in km")).toBeNull();
    expect(convert("10 apples in pears")).toBeNull();
  });

  test("large and small numbers use an exponent", () => {
    expect(formatNumber(2 ** 60)).toBe("1.152922e+18");
    expect(formatNumber(1e-9)).toBe("1e-9");
  });
});
