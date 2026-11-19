/** The flow control and the speed measurement of the terminal app. */

import { PTY_WINDOW } from "../../proto";

/** The shell acknowledges output in steps of this size, or after a short time. */
export const ACK_STEP = PTY_WINDOW / 4;

/** The wait before a small acknowledgment goes. */
export const ACK_DELAY_MS = 20;

/**
 * Counts the output bytes that the emulator parsed, and sends an acknowledgment
 * each `ACK_STEP` bytes, or `ACK_DELAY_MS` after the last parsed output.
 */
export class AckCounter {
  #pending = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly send: (bytes: number) => void) {}

  /** Records `bytes` parsed bytes. */
  parsed(bytes: number): void {
    this.#pending += bytes;
    if (this.#pending >= ACK_STEP) {
      this.flush();
    } else if (this.#timer === undefined) {
      this.#timer = setTimeout(() => this.flush(), ACK_DELAY_MS);
    }
  }

  /** Sends the acknowledgment of all parsed bytes now. */
  flush(): void {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    if (this.#pending > 0) {
      this.send(this.#pending);
      this.#pending = 0;
    }
  }

  dispose(): void {
    clearTimeout(this.#timer);
  }
}

/** The name of the measurement of the echo delay in the Performance API. */
export const ECHO_MEASURE = "farglass:terminal-echo";

/**
 * Measures the delay from a key press to its echo: from the input that the
 * emulator gives, to the parse of the next output. Each measurement is a
 * `performance.measure` with the name `ECHO_MEASURE`.
 */
export class EchoProbe {
  #start: number | null = null;

  /** Records the time of an input. An input that waits for its echo keeps the earlier time. */
  input(now = performance.now()): void {
    this.#start ??= now;
  }

  /** Records the parse of output. Returns the delay, or `null` if no input waited. */
  output(now = performance.now()): number | null {
    if (this.#start === null) {
      return null;
    }
    const start = this.#start;
    this.#start = null;
    performance.measure(ECHO_MEASURE, { start, end: now });
    return now - start;
  }
}
