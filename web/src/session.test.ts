// @vitest-environment node
import { expect, test } from "vitest";
import { storeSession } from "./session";

test("storeSession posts the ticket as JSON with the cookie of this origin", async () => {
  const calls: [string, RequestInit][] = [];
  const fetcher = ((url: string, init: RequestInit) => {
    calls.push([url, init]);
    return Promise.resolve(new Response(null, { status: 204 }));
  }) as typeof fetch;
  expect(await storeSession("abc", fetcher)).toBe(true);
  const [url, init] = calls[0]!;
  expect(url).toBe("/session");
  expect(init.method).toBe("POST");
  expect(init.credentials).toBe("same-origin");
  expect(JSON.parse(init.body as string)).toEqual({ ticket: "abc" });
});

test("storeSession reports a refusal and a network error", async () => {
  const refused = (() => Promise.resolve(new Response(null, { status: 403 }))) as unknown as typeof fetch;
  expect(await storeSession("abc", refused)).toBe(false);
  const broken = (() => Promise.reject(new TypeError("network"))) as unknown as typeof fetch;
  expect(await storeSession("abc", broken)).toBe(false);
});
