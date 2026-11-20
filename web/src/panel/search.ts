/** The search of the launcher. */

/** An entry that the launcher can show. */
export interface Searchable {
  readonly id: string;
  readonly title: string;
  readonly keywords?: readonly string[] | undefined;
}

/**
 * Returns the entries that match `query`, the best match first.
 *
 * Each word of the query must be in the title, the id, or a keyword, with no
 * difference between uppercase and lowercase. A title that starts with the
 * query comes first, then a word of the title that starts with it, then all
 * other matches. Equal matches stay in title order. An empty query gives all entries.
 */
export function search<T extends Searchable>(entries: readonly T[], query: string): T[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const sorted = [...entries].sort((a, b) => a.title.localeCompare(b.title));
  if (words.length === 0) {
    return sorted;
  }
  const scored = sorted.flatMap((entry) => {
    const title = entry.title.toLowerCase();
    const text = [title, entry.id.toLowerCase(), ...(entry.keywords ?? []).map((k) => k.toLowerCase())].join(" ");
    if (!words.every((word) => text.includes(word))) {
      return [];
    }
    const phrase = words.join(" ");
    const score = title.startsWith(phrase) ? 0 : title.split(/\s+/).some((w) => w.startsWith(words[0]!)) ? 1 : 2;
    return [{ entry, score }];
  });
  return scored.sort((a, b) => a.score - b.score).map(({ entry }) => entry);
}
