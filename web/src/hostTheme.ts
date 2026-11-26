/**
 * The dark or light theme of the shell for the apps of the host (F120, ADR-059).
 *
 * The shell keeps a theme channel open while the desktop shows. It sends the
 * theme that it shows at the start and after each change: dark for Dark and
 * LCARS, and the setting of the browser for System. The agent then sets the
 * theme of the GTK and KDE apps of the web session.
 */

import { isDarkTheme, onThemeChange } from "./appearance";
import type { Connection, ThemeRequest } from "./proto";

/** Opens the theme channel for `connection`. Returns the function that closes it. */
export function startHostTheme(connection: Connection): () => void {
  const channel = connection.openChannel({ kind: "theme" }, {});
  let sent: boolean | undefined;
  const send = () => {
    const dark = isDarkTheme();
    if (dark === sent) return;
    sent = dark;
    channel.control({ type: "color-scheme", dark } satisfies ThemeRequest);
  };
  send();
  const stop = onThemeChange(send);
  return () => {
    stop();
    channel.close();
  };
}
