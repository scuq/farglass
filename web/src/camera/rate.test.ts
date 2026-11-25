import { describe, expect, test } from "vitest";
import { encoderBitrate, frameRate, window, initialRate, MAX_BITRATE, MIN_BITRATE, nextRate, type RateState } from "./rate";

/**
 * Simulates the camera on a link: a base round trip of 20 ms, and an uplink of `capacity` bit/s that sends one picture
 * after the other. The encoder spends its bit rate on the pictures of the last second. A picture goes only when fewer
 * pictures than the window of the state wait for their ack, and at most at the frame rate of the state.
 * Returns the state, and the largest extra delay that a key press behind the camera got in the last 20 s.
 */
function run(state: RateState, capacity: (now: number) => number, seconds: number, start = 0) {
  const acks: { at: number; sent: number; bits: number; alone: boolean }[] = [];
  let count = 0;
  // A fixed sequence of sizes: a P-frame differs by up to 30 %, and a key frame is 5 times larger. The shell asks for a
  // key frame at the start of a stream and after a loss: here one each 60 s.
  let seed = 1;
  const random = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  const encoded: number[] = [];
  let linkFree = start;
  let next = start;
  let worst = 0;
  for (let now = start; now < start + seconds * 1000; now += 1) {
    while (acks.length && acks[0]!.at <= now) {
      const ack = acks.shift()!;
      state = nextRate(state, ack);
    }
    if (now > start + (seconds - 20) * 1000) worst = Math.max(worst, linkFree - now);
    if (now < next || acks.length >= window(state)) continue;
    next = now + 1000 / frameRate(state);
    while (encoded.length && now - encoded[0]! > 1000) encoded.shift();
    encoded.push(now);
    const size = count++ % 1800 === 0 ? 5 : 0.7 + 0.6 * random();
    const bits = (encoderBitrate(state) / Math.max(encoded.length, 1)) * size;
    linkFree = Math.max(linkFree, now) + (bits / capacity(now)) * 1000;
    acks.push({ at: linkFree + 20, sent: now, bits, alone: acks.length === 0 });
  }
  return { state, worst, bitrate: encoderBitrate(state), frames: frameRate(state) };
}

describe("the rate of the camera", () => {
  test("a fast uplink keeps the highest rate", () => {
    const { state, worst } = run(initialRate(0), () => 100_000_000, 60);
    const { bitrate, frames } = run(initialRate(0), () => 100_000_000, 60);
    expect(bitrate).toBe(MAX_BITRATE);
    expect(frames).toBe(30);
    expect(worst).toBeLessThan(5);
    expect(state.bitrate).toBe(MAX_BITRATE);
  });

  test("a slow uplink lowers the bit rate, and a key press waits less than 60 ms behind the camera", () => {
    const { bitrate, worst } = run(initialRate(0), () => 600_000, 120);
    expect(bitrate).toBeLessThan(600_000);
    expect(worst).toBeLessThan(60);
  });

  test("an uplink below the lowest bit rate lowers the frame rate", () => {
    const { bitrate, frames, worst } = run(initialRate(0), () => 120_000, 240);
    expect(bitrate).toBeLessThan(MIN_BITRATE);
    expect(frames).toBeLessThan(30);
    expect(worst).toBeLessThan(100);
  });

  test("when the uplink is fast again, the frame rate and the bit rate come back", () => {
    const { bitrate, frames } = run(initialRate(0), (now) => (now < 240_000 ? 120_000 : 100_000_000), 480);
    expect(frames).toBe(30);
    expect(bitrate).toBe(MAX_BITRATE);
  });

  test("a software encoder keeps its lower highest frame rate", () => {
    const { frames } = run(initialRate(0, 15), () => 100_000_000, 60);
    expect(frames).toBe(15);
  });
});

test("the worst wait of a key press behind the camera, for each uplink", () => {
  for (const capacity of [10_000_000, 3_000_000, 1_000_000, 600_000, 300_000, 120_000]) {
    const { worst, bitrate, frames } = run(initialRate(0), () => capacity, 120);
    console.log(`uplink ${capacity / 1000} kbit/s: camera ${Math.round(bitrate / 1000)} kbit/s at ${frames} fps, worst wait ${Math.round(worst)} ms`);
    // A key press waits behind one P-frame at most: about 25 ms at half of the uplink and 30 pictures each second.
    expect(worst).toBeLessThan(capacity >= 300_000 ? 40 : 100);
  }
});
