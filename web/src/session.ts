/**
 * The session cookie. After a login, the shell gives the ticket to farglassd,
 * and the answer sets an HttpOnly cookie. The shell never sees the cookie.
 */

/**
 * Gives the ticket of a login to `POST /session`. Returns `true` if farglassd set the cookie.
 *
 * Without the cookie, the current connection keeps its user, but a reload needs a new login.
 */
export async function storeSession(ticket: string, fetcher: typeof fetch = fetch): Promise<boolean> {
  try {
    const response = await fetcher("/session", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ticket }),
      credentials: "same-origin",
    });
    return response.ok;
  } catch {
    return false;
  }
}
