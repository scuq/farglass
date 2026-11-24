/**
 * Links that leave the host: a new tab of the client browser (F016, F100).
 *
 * The tab opens on the computer of the user, not on the host. Call
 * `openInClient` in the handler of a click or a key: a browser blocks a new
 * tab that no user action asked for.
 */

/** Reports whether `url` is an http or https URL with a host. Only these leave the host. */
export function isWebUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return (parsed.protocol === "http:" || parsed.protocol === "https:") && parsed.hostname !== "";
  } catch {
    return false;
  }
}

/** Opens `url` in a new tab of the client browser. Returns false if the URL is not http or https. */
export function openInClient(url: string): boolean {
  if (!isWebUrl(url)) return false;
  // noopener: the new tab gets no handle to the shell.
  window.open(url, "_blank", "noopener,noreferrer");
  return true;
}
