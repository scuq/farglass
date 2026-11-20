import { describe, expect, test } from "vitest";
import { creationOptions, fromBase64url, requestOptions, toBase64url } from "./passkey";

describe("passkey JSON (F034)", () => {
  test("base64url goes both ways with no padding", () => {
    for (const bytes of [[], [0], [251, 255], [1, 2, 3, 4, 5]]) {
      const text = toBase64url(new Uint8Array(bytes));
      expect(text).not.toMatch(/[+/=]/);
      expect([...new Uint8Array(fromBase64url(text))]).toEqual(bytes);
    }
  });

  test("the options get their binary values as bytes", () => {
    const create = creationOptions({
      challenge: "AQID",
      rp: { id: "farglass.cyberdyne.example", name: "farglass" },
      user: { id: "BAU", name: "alice", displayName: "alice" },
      pubKeyCredParams: [{ type: "public-key", alg: -7 }],
      excludeCredentials: [{ type: "public-key", id: "Bg" }],
    });
    expect([...new Uint8Array(create.challenge as ArrayBuffer)]).toEqual([1, 2, 3]);
    expect([...new Uint8Array(create.user.id as ArrayBuffer)]).toEqual([4, 5]);
    expect([...new Uint8Array(create.excludeCredentials![0]!.id as ArrayBuffer)]).toEqual([6]);
    const get = requestOptions({ challenge: "AQID", allowCredentials: [{ type: "public-key", id: "Bg" }] });
    expect([...new Uint8Array(get.allowCredentials![0]!.id as ArrayBuffer)]).toEqual([6]);
  });
});
