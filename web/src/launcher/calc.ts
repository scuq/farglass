/**
 * The calculator of the launcher (F042): expressions and unit conversions, in the client browser.
 *
 * The parser reads numbers, + - * / % ^, parentheses, the constants pi and e, and the functions
 * sqrt, abs, sin, cos, tan, asin, acos, atan, ln, log, log2, exp, round, floor, and ceil.
 * It does not use eval. A conversion is "VALUE UNIT in UNIT", also with "to", "as", or "=".
 */

const FUNCTIONS: Readonly<Record<string, (x: number) => number>> = {
  sqrt: Math.sqrt,
  abs: Math.abs,
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  asin: Math.asin,
  acos: Math.acos,
  atan: Math.atan,
  ln: Math.log,
  log: Math.log10,
  log2: Math.log2,
  exp: Math.exp,
  round: Math.round,
  floor: Math.floor,
  ceil: Math.ceil,
};

const CONSTANTS: Readonly<Record<string, number>> = { pi: Math.PI, π: Math.PI, e: Math.E };

type Token = { kind: "number"; value: number } | { kind: "name"; value: string } | { kind: "op"; value: string };

function tokenize(text: string): Token[] | null {
  const tokens: Token[] = [];
  const source = text.replaceAll("×", "*").replaceAll("÷", "/").replaceAll("−", "-").replaceAll("**", "^");
  let i = 0;
  while (i < source.length) {
    const c = source[i]!;
    if (/\s/.test(c)) {
      i++;
    } else if (/[\d.]/.test(c)) {
      const match = /^(?:\d[\d_]*(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?/i.exec(source.slice(i));
      if (!match) return null;
      tokens.push({ kind: "number", value: Number(match[0].replaceAll("_", "")) });
      i += match[0].length;
    } else if (/[\p{L}_]/u.test(c)) {
      const match = /^[\p{L}_][\p{L}\d_]*/u.exec(source.slice(i))!;
      tokens.push({ kind: "name", value: match[0].toLowerCase() });
      i += match[0].length;
    } else if ("+-*/%^()".includes(c)) {
      tokens.push({ kind: "op", value: c });
      i++;
    } else {
      return null;
    }
  }
  return tokens;
}

/** A recursive descent parser. Each method returns the value, or throws for a text that is not an expression. */
class Parser {
  #at = 0;
  /** Reports whether the expression has an operator or a function: a plain number is not a calculation. */
  computed = false;

  constructor(readonly tokens: Token[]) {}

  #peek(): Token | undefined {
    return this.tokens[this.#at];
  }

  #op(value: string): boolean {
    const token = this.#peek();
    if (token?.kind === "op" && token.value === value) {
      this.#at++;
      return true;
    }
    return false;
  }

  parse(): number {
    const value = this.#sum();
    if (this.#at !== this.tokens.length) throw new Error("rest");
    return value;
  }

  #sum(): number {
    let value = this.#product();
    for (;;) {
      if (this.#op("+")) value += this.#product();
      else if (this.#op("-")) value -= this.#product();
      else return value;
      this.computed = true;
    }
  }

  #product(): number {
    let value = this.#unary();
    for (;;) {
      if (this.#op("*")) value *= this.#unary();
      else if (this.#op("/")) value /= this.#unary();
      else if (this.#op("%")) value %= this.#unary();
      else return value;
      this.computed = true;
    }
  }

  #unary(): number {
    if (this.#op("-")) {
      return -this.#unary();
    }
    if (this.#op("+")) return this.#unary();
    return this.#power();
  }

  #power(): number {
    const base = this.#primary();
    if (this.#op("^")) {
      this.computed = true;
      // Right associative, and the exponent can have a sign: 2^-1.
      return base ** this.#unary();
    }
    return base;
  }

  #primary(): number {
    const token = this.#peek();
    if (!token) throw new Error("end");
    this.#at++;
    if (token.kind === "number") return token.value;
    if (token.kind === "op" && token.value === "(") {
      const value = this.#sum();
      if (!this.#op(")")) throw new Error(")");
      return value;
    }
    if (token.kind === "name") {
      const fn = FUNCTIONS[token.value];
      if (fn) {
        if (!this.#op("(")) throw new Error("(");
        const value = this.#sum();
        if (!this.#op(")")) throw new Error(")");
        this.computed = true;
        return fn(value);
      }
      const constant = CONSTANTS[token.value];
      if (constant !== undefined) return constant;
    }
    throw new Error("token");
  }
}

/** Returns the value of an expression, or null if the text is not an expression. `computed` is false for a plain number or constant. */
export function evaluate(text: string): { value: number; computed: boolean } | null {
  const tokens = tokenize(text);
  if (!tokens || tokens.length === 0) return null;
  const parser = new Parser(tokens);
  try {
    const value = parser.parse();
    return Number.isFinite(value) ? { value, computed: parser.computed } : null;
  } catch {
    return null;
  }
}

/** Formats a result with at most 12 significant digits and no trailing zeros. */
export function formatNumber(value: number): string {
  if (Object.is(value, -0) || Math.abs(value) < 1e-15) return "0";
  const abs = Math.abs(value);
  if (abs >= 1e15 || abs < 1e-6) return value.toExponential(6).replace(/\.?0+e/, "e");
  return String(Number(value.toPrecision(12)));
}

/** A unit: its dimension, its factor to the base unit of the dimension, and its name for the result. */
interface Unit {
  readonly dimension: string;
  readonly factor: number;
  readonly name: string;
}

const UNITS = new Map<string, Unit>();
const unit = (dimension: string, factor: number, name: string, ...aliases: string[]) => {
  for (const alias of [name, ...aliases]) UNITS.set(alias.toLowerCase(), { dimension, factor, name });
};
unit("length", 1, "m", "meter", "meters", "metre", "metres");
unit("length", 1000, "km", "kilometer", "kilometers");
unit("length", 0.01, "cm", "centimeter", "centimeters");
unit("length", 0.001, "mm", "millimeter", "millimeters");
unit("length", 1e-6, "µm", "um", "micrometer");
unit("length", 1e-9, "nm", "nanometer");
unit("length", 1609.344, "mi", "mile", "miles");
unit("length", 0.9144, "yd", "yard", "yards");
unit("length", 0.3048, "ft", "foot", "feet");
unit("length", 0.0254, "in", "inch", "inches");
unit("length", 1852, "nmi", "nautical mile");
unit("mass", 1, "kg", "kilogram", "kilograms");
unit("mass", 0.001, "g", "gram", "grams");
unit("mass", 1e-6, "mg", "milligram");
unit("mass", 1000, "t", "tonne", "tonnes");
unit("mass", 0.45359237, "lb", "lbs", "pound", "pounds");
unit("mass", 0.028349523125, "oz", "ounce", "ounces");
unit("mass", 6.35029318, "st", "stone");
unit("time", 1, "s", "sec", "second", "seconds");
unit("time", 0.001, "ms", "millisecond", "milliseconds");
unit("time", 1e-6, "µs", "us", "microsecond");
unit("time", 60, "min", "minute", "minutes");
unit("time", 3600, "h", "hr", "hour", "hours");
unit("time", 86400, "d", "day", "days");
unit("time", 604800, "wk", "week", "weeks");
unit("time", 31557600, "yr", "year", "years");
unit("data", 1, "B", "byte", "bytes");
unit("data", 0.125, "bit", "bits");
unit("data", 1e3, "kB", "kb");
unit("data", 1e6, "MB", "mb");
unit("data", 1e9, "GB", "gb");
unit("data", 1e12, "TB", "tb");
unit("data", 1024, "KiB", "kib");
unit("data", 1024 ** 2, "MiB", "mib");
unit("data", 1024 ** 3, "GiB", "gib");
unit("data", 1024 ** 4, "TiB", "tib");
unit("data", 125, "kbit", "kbps");
unit("data", 125_000, "Mbit", "mbps");
unit("data", 125_000_000, "Gbit", "gbps");
unit("speed", 1, "m/s", "mps");
unit("speed", 1 / 3.6, "km/h", "kmh", "kph");
unit("speed", 0.44704, "mph");
unit("speed", 1852 / 3600, "kn", "knot", "knots");
unit("volume", 1, "l", "liter", "liters", "litre", "litres");
unit("volume", 0.001, "ml", "milliliter");
unit("volume", 0.01, "cl");
unit("volume", 0.1, "dl");
unit("volume", 1000, "m³", "m3");
unit("volume", 3.785411784, "gal", "gallon", "gallons");
unit("volume", 0.946352946, "qt", "quart", "quarts");
unit("volume", 0.473176473, "pt", "pint", "pints");
unit("volume", 0.2365882365, "cup", "cups");
unit("volume", 0.0295735295625, "fl oz", "floz");
unit("area", 1, "m²", "m2");
unit("area", 1e6, "km²", "km2");
unit("area", 1e-4, "cm²", "cm2");
unit("area", 1e4, "ha", "hectare", "hectares");
unit("area", 4046.8564224, "acre", "acres");
unit("area", 0.09290304, "ft²", "ft2", "sqft");
unit("angle", 1, "rad", "radian", "radians");
unit("angle", Math.PI / 180, "°", "deg", "degree", "degrees");

/** The temperatures: to Kelvin and back. */
const TEMPERATURES: Readonly<Record<string, { name: string; toK: (v: number) => number; fromK: (k: number) => number }>> = {
  c: { name: "°C", toK: (v) => v + 273.15, fromK: (k) => k - 273.15 },
  f: { name: "°F", toK: (v) => ((v - 32) * 5) / 9 + 273.15, fromK: (k) => ((k - 273.15) * 9) / 5 + 32 },
  k: { name: "K", toK: (v) => v, fromK: (k) => k },
};
const temperature = (name: string) =>
  TEMPERATURES[name.toLowerCase().replace(/^°/, "").replace(/^(celsius)$/, "c").replace(/^(fahrenheit)$/, "f").replace(/^(kelvin)$/, "k")];

/** Returns "VALUE FROM = RESULT TO" for a conversion, or null if the text is not one. */
export function convert(text: string): { text: string; value: string } | null {
  const match = /^(.+?)\s*([^\s\d().+*/^%-][^\s]*(?: oz)?)\s+(?:in|to|as|=|->)\s+(\S+(?: oz)?)\s*$/i.exec(text.trim());
  if (!match) return null;
  const amount = evaluate(match[1]!);
  if (!amount) return null;
  const [from, to] = [match[2]!, match[3]!];
  const show = (value: number, fromName: string, toName: string) => {
    const result = formatNumber(value);
    return { text: `${formatNumber(amount.value)} ${fromName} = ${result} ${toName}`, value: result };
  };
  const tFrom = temperature(from);
  const tTo = temperature(to);
  if (tFrom && tTo) return show(tTo.fromK(tFrom.toK(amount.value)), tFrom.name, tTo.name);
  const a = UNITS.get(from.toLowerCase());
  const b = UNITS.get(to.toLowerCase());
  if (!a || !b || a.dimension !== b.dimension) return null;
  return show((amount.value * a.factor) / b.factor, a.name, b.name);
}

/** A result of the calculator for the launcher. */
export interface CalcResult {
  /** The line to show, for example "2 * 21 = 42". */
  readonly text: string;
  /** The value to copy. */
  readonly value: string;
}

/** Returns the result of the query as a calculation or a conversion, or null. A plain number is no result. */
export function calculate(query: string): CalcResult | null {
  const text = query.trim();
  if (!text) return null;
  const conversion = convert(text);
  if (conversion) return conversion;
  const result = evaluate(text);
  if (!result || !result.computed) return null;
  const value = formatNumber(result.value);
  return { text: `${text} = ${value}`, value };
}
