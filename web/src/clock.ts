/**
 * The clock of the panel (F099): the hour format and the optional crockford timestamp.
 * The settings app saves the settings in the state store under {@link CLOCK_KEY},
 * thus each tab and each login of the user gets them.
 */

/** The key in the state store. */
export const CLOCK_KEY = "clock";

/** The hour formats: the format of the language of the browser, 24 hours, or 12 hours with AM and PM. */
export const HOUR_FORMATS = ["auto", "24", "12"] as const;

export interface ClockSettings {
  readonly hours: (typeof HOUR_FORMATS)[number];
  /** The panel also shows the time as a crockford timestamp. */
  readonly crockford: boolean;
}

export const DEFAULT_CLOCK: ClockSettings = { hours: "auto", crockford: false };

/** Returns valid settings from a value of the state store. Unknown fields get the default. */
export function parseClockSettings(value: unknown): ClockSettings {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  return {
    hours: HOUR_FORMATS.includes(record.hours as ClockSettings["hours"]) ? (record.hours as ClockSettings["hours"]) : DEFAULT_CLOCK.hours,
    crockford: typeof record.crockford === "boolean" ? record.crockford : DEFAULT_CLOCK.crockford,
  };
}

/** Returns the options of Intl.DateTimeFormat for the hour format. */
export function hourOptions(hours: ClockSettings["hours"]): Intl.DateTimeFormatOptions {
  switch (hours) {
    case "24":
      return { hourCycle: "h23" };
    case "12":
      return { hourCycle: "h12" };
    default:
      return {};
  }
}

/** Returns the time of the panel: the hour and the minute in the hour format. */
export function formatClock(date: Date, hours: ClockSettings["hours"], locale?: string): string {
  return new Intl.DateTimeFormat(locale, { hour: hours === "12" ? "numeric" : "2-digit", minute: "2-digit", ...hourOptions(hours) }).format(date);
}

/** The Crockford Base32 alphabet: no I, L, O, or U. */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * Returns the crockford timestamp of `date`, or null for a year before 2020 or after 3043.
 *
 * The code has one character each for the year (year - 2020, mod 32), the month,
 * the day, the hour, and the minute divided by 2, all in UTC. A year after 2051
 * adds the cycle as `+N`. See https://github.com/scuq/crockford-timestamp.
 */
export function crockfordTimestamp(date: Date): string | null {
  const offset = date.getUTCFullYear() - 2020;
  if (!Number.isFinite(offset) || offset < 0 || offset > 1023) return null;
  const cycle = Math.floor(offset / 32);
  const code = [offset % 32, date.getUTCMonth() + 1, date.getUTCDate(), date.getUTCHours(), Math.floor(date.getUTCMinutes() / 2)]
    .map((value) => ALPHABET[value])
    .join("");
  return cycle === 0 ? code : `${code}+${ALPHABET[cycle]}`;
}

/** Returns the UTC start of the 2-minute interval of `date`, as `YYYY-MM-DD HH:MM`: the time that its crockford timestamp stands for. */
export function crockfordStart(date: Date): string {
  const start = new Date(date);
  start.setUTCSeconds(0, 0);
  start.setUTCMinutes(start.getUTCMinutes() - (start.getUTCMinutes() % 2));
  return start.toISOString().slice(0, 16).replace("T", " ");
}
