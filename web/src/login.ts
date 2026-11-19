/**
 * The login on a channel of kind login. farglassd relays each PAM event.
 */

import type { Close, Connection, LocalSession, LoginEvent, LoginReply } from "./proto";

/** The callbacks of one login. */
export interface LoginHandlers {
  /** A PAM event arrived: a prompt, a message, or the result. */
  onevent: (event: LoginEvent) => void;
  /** The channel closed before a result, for example after an error of the login service. */
  onstop: (close: Close) => void;
}

/** One login attempt. */
export class Login {
  #result = false;
  readonly #channel;

  constructor(connection: Connection, handlers: LoginHandlers) {
    this.#channel = connection.openChannel(
      { kind: "login" },
      {
        oncontrol: (message) => {
          const event = message as LoginEvent;
          if (event.type === "success" || event.type === "failure" || event.type === "blocked") {
            this.#result = true;
          }
          handlers.onevent(event);
        },
        onclose: (close) => {
          if (!this.#result) {
            handlers.onstop(close);
          }
        },
      },
    );
  }

  /** Answers the prompt with the number `prompt`. */
  answer(prompt: number, text: string): void {
    const reply: LoginReply = { type: "answer", prompt, text };
    this.#channel.control(reply);
  }

  /** Answers a confirm event: `true` ends the local sessions, `false` stops the login. */
  confirm(accept: boolean): void {
    const reply: LoginReply = { type: "confirm", accept };
    this.#channel.control(reply);
  }

  /** Stops the attempt. */
  cancel(): void {
    this.#channel.close({ reason: "cancelled" });
  }
}

/** Returns the label of a PAM prompt: the text with no colon and no space at the end. */
export function promptLabel(text: string): string {
  const label = text.trim().replace(/:$/, "").trim();
  return label === "" ? "Answer" : label.charAt(0).toUpperCase() + label.slice(1);
}

/**
 * Returns the value of the HTML attribute autocomplete for a prompt.
 *
 * A password manager then fills the username and the password, and the browser offers a one-time code.
 */
export function promptAutocomplete(text: string, echo: boolean): string {
  if (/\b(code|otp|token|verification)\b/i.test(text)) {
    return "one-time-code";
  }
  return echo ? "username" : "current-password";
}

/** Returns a short description of a local session, for example "a Wayland session on seat0". */
export function describeSession(session: LocalSession): string {
  const kinds: Record<string, string> = { wayland: "a Wayland", x11: "an X11", mir: "a Mir" };
  const kind = kinds[session.kind] ?? `a ${session.kind}`;
  return session.seat ? `${kind} session on ${session.seat}` : `${kind} session`;
}
