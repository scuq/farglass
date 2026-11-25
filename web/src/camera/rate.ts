/**
 * The rate of the camera: the bit rate and the frame rate that keep the desktop fast (F118, ADR-058).
 *
 * The agent acks each picture, and at most two pictures wait for their ack.
 * Two signals come from the acks:
 *
 * - The uplink. A picture that goes alone has a round trip of the base round
 *   trip plus its size divided by the uplink. A line through the round trips
 *   and the sizes of the last pictures gives the uplink. Also, when a picture
 *   left before the ack of the picture in front of it, the time between their
 *   acks is the time of the link for the second picture. The camera uses at
 *   most half of the uplink, thus a key press behind a picture waits for a
 *   short time only.
 * - A queue of other traffic. When the median round trip of the last pictures
 *   is clearly above the minimum of the last seconds, the camera lowers its
 *   bit rate by a quarter. After a calm time, it raises it again.
 *
 * Below the lowest bit rate, the frame rate goes down.
 */

/** The highest bit rate. */
export const MAX_BITRATE = 1_500_000;

/** The bit rate at the start of a stream. It goes up while the link has room. */
export const START_BITRATE = 500_000;

/** The lowest bit rate at the full frame rate. Below it, the frame rate goes down. */
export const MIN_BITRATE = 250_000;

/** The frame rates, the highest first. */
export const FRAME_RATES = [30, 15, 10, 5] as const;

/** The extra round trip above the minimum that shows a queue, in ms. */
export const QUEUE_MS = 30;

/** The part of the uplink that the camera uses at most. */
export const UPLINK_SHARE = 0.5;

/** The pictures of the median. */
const SAMPLES = 15;

/** The time of the minimum round trip, in ms. */
const MIN_WINDOW_MS = 30_000;

/** The time with no queue before the bit rate goes up, in ms. */
const CALM_MS = 5_000;

/** The shortest time between two changes of the delay signal, in ms. */
const CHANGE_MS = 3_000;

/** The weight of a new uplink sample. A lower sample counts at once, so a slower link gets a lower rate fast. */
const UPLINK_WEIGHT = 0.2;

/** The pictures alone on the link for the line of the uplink. */
const FIT_SAMPLES = 60;

/** The time of one picture on the link above which only one picture can wait for its ack, in ms. */
const SHORT_PICTURE_MS = 10;

/** The growth of the uplink for each ack of a picture that did not wait: the camera finds a faster link again. */
const UPLINK_GROWTH = 1.005;

/** One ack: the time of the encode, the time of the ack, and the size of the picture. */
export interface RateSample {
  readonly sent: number;
  readonly at: number;
  readonly bits: number;
  /** Reports whether no other picture waited for its ack when this one went. */
  readonly alone: boolean;
}

export interface RateState {
  /** The bit rate of the delay signal. */
  readonly bitrate: number;
  /** The highest frame rate: the format, or less for a software encoder. */
  readonly maxFrameRate: number;
  /** The uplink in bit/s, or null before the first sample. */
  readonly uplink: number | null;
  /** The time of the last ack. */
  readonly lastAck: number | null;
  /** The sizes and the round trips of the last pictures that went alone. */
  readonly alone: readonly { readonly bits: number; readonly rtt: number }[];
  /** The round trips of the last seconds, with their times. */
  readonly history: readonly { readonly at: number; readonly rtt: number }[];
  /** The round trips for the median, the newest last. */
  readonly recent: readonly number[];
  readonly lastChange: number;
  readonly calmSince: number;
}

/** Returns the state at the start of a stream: the highest rate. */
export function initialRate(now: number, maxFrameRate: number = FRAME_RATES[0]): RateState {
  return {
    bitrate: START_BITRATE,
    maxFrameRate,
    uplink: null,
    lastAck: null,
    alone: [],
    history: [],
    recent: [],
    lastChange: now,
    calmSince: now,
  };
}

/** Returns the uplink in bit/s from the line through round trips and sizes, or null if the sizes vary too little. */
export function fitUplink(samples: readonly { readonly bits: number; readonly rtt: number }[]): number | null {
  if (samples.length < 10) return null;
  const n = samples.length;
  const meanBits = samples.reduce((sum, s) => sum + s.bits, 0) / n;
  const meanRtt = samples.reduce((sum, s) => sum + s.rtt, 0) / n;
  let cov = 0;
  let varBits = 0;
  for (const sample of samples) {
    cov += (sample.bits - meanBits) * (sample.rtt - meanRtt);
    varBits += (sample.bits - meanBits) ** 2;
  }
  // The sizes must differ by 10 % or more, else the line has no slope.
  if (varBits / n < (0.1 * meanBits) ** 2) return null;
  const slope = cov / varBits;
  // A flat line: the pictures take no time that counts on the link.
  if (slope <= 0) return Infinity;
  return 1000 / slope;
}

function median(values: readonly number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
}

/** Returns the bit rate that the camera can use now: the delay signal, at most the share of the uplink. */
function allowed(state: RateState): number {
  return state.uplink === null ? state.bitrate : Math.min(state.bitrate, state.uplink * UPLINK_SHARE);
}

/**
 * Returns the pictures that can wait for their ack: two on a fast link, so the round trip does not limit the frame rate,
 * and one when a picture takes a long time on the link, so a key press waits behind one picture at most.
 */
export function window(state: RateState): 1 | 2 {
  if (state.uplink === null) return 1;
  const picture = encoderBitrate(state) / frameRate(state);
  return (picture * 1000) / state.uplink <= SHORT_PICTURE_MS ? 2 : 1;
}

/** Returns the frame rate: the highest one, or a lower one when the allowed bit rate is below the lowest bit rate. */
export function frameRate(state: RateState): number {
  const bits = allowed(state);
  if (bits >= MIN_BITRATE) return state.maxFrameRate;
  const rate = FRAME_RATES.find((rate) => rate <= state.maxFrameRate && (MIN_BITRATE * rate) / FRAME_RATES[0] <= bits);
  return rate ?? FRAME_RATES[3];
}

/**
 * Returns the bit rate for the encoder. Below the full frame rate, it goes down with the frame rate:
 * the encoder spends its bit rate on the pictures that it gets, thus fewer pictures alone do not lower the load.
 */
export function encoderBitrate(state: RateState): number {
  const bits = allowed(state);
  if (bits >= MIN_BITRATE) return Math.round(bits);
  return Math.round((MIN_BITRATE * frameRate(state)) / FRAME_RATES[0]);
}

/** Returns the state after one ack. */
export function nextRate(state: RateState, sample: RateSample): RateState {
  const now = sample.at;
  let uplink = state.uplink;
  const take = (measured: number) => {
    uplink = uplink === null || measured < uplink ? measured : uplink + (measured - uplink) * UPLINK_WEIGHT;
  };
  const alone = sample.alone ? [...state.alone, { bits: sample.bits, rtt: sample.at - sample.sent }].slice(-FIT_SAMPLES) : state.alone;
  const fitted = sample.alone ? fitUplink(alone) : null;
  if (state.lastAck !== null && sample.sent < state.lastAck && now > state.lastAck) {
    // The picture waited behind the one before it: the time between the two acks is its time on the link.
    take((sample.bits * 1000) / (now - state.lastAck));
  } else if (fitted !== null && Number.isFinite(fitted)) {
    take(fitted);
  } else if (uplink !== null) {
    uplink = Math.min(uplink * UPLINK_GROWTH, (MAX_BITRATE / UPLINK_SHARE) * 2);
  }
  // The time of this picture on the link is not a queue: the uplink rule takes care of it.
  const own = uplink === null ? 0 : (sample.bits * 1000) / uplink;
  const rtt = Math.max(0, sample.at - sample.sent - own);
  const history = [...state.history.filter((old) => now - old.at <= MIN_WINDOW_MS), { at: now, rtt }];
  const recent = [...state.recent, rtt].slice(-SAMPLES);
  const next: RateState = { ...state, uplink, lastAck: now, alone, history, recent };
  if (recent.length < SAMPLES) return next;
  const minimum = Math.min(...history.map((old) => old.rtt));
  const queue = median(recent) - minimum > QUEUE_MS;
  const canChange = now - state.lastChange >= CHANGE_MS;
  if (queue) {
    if (!canChange) return { ...next, calmSince: now };
    return { ...next, bitrate: Math.max(MIN_BITRATE / 4, Math.round(state.bitrate * 0.75)), recent: [], lastChange: now, calmSince: now };
  }
  if (!canChange || now - state.calmSince < CALM_MS || state.bitrate >= MAX_BITRATE) return next;
  return { ...next, bitrate: Math.min(MAX_BITRATE, Math.round(state.bitrate * 1.1)), lastChange: now };
}
