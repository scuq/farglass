/** Decodes one LZ4 block (the format of lz4_flex::block) into `size` bytes. */
export function decodeLz4Block(src: Uint8Array, size: number): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(size);
  let s = 0;
  let d = 0;
  while (s < src.length) {
    const token = src[s++]!;
    let literals = token >>> 4;
    if (literals === 15) {
      let byte: number;
      do {
        byte = src[s++]!;
        literals += byte;
      } while (byte === 255);
    }
    out.set(src.subarray(s, s + literals), d);
    s += literals;
    d += literals;
    if (s >= src.length) {
      break;
    }
    const offset = src[s]! | (src[s + 1]! << 8);
    s += 2;
    let length = token & 15;
    if (length === 15) {
      let byte: number;
      do {
        byte = src[s++]!;
        length += byte;
      } while (byte === 255);
    }
    length += 4;
    if (offset === 0 || offset > d) {
      throw new Error("The LZ4 block is not valid.");
    }
    // The match can overlap the output, thus the copy goes byte by byte when it is near.
    let from = d - offset;
    if (offset >= length) {
      out.copyWithin(d, from, from + length);
      d += length;
    } else {
      for (let i = 0; i < length; i++) {
        out[d++] = out[from++]!;
      }
    }
  }
  if (d !== size) {
    throw new Error(`The LZ4 block has ${d} bytes, not ${size}.`);
  }
  return out;
}
