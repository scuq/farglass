import { PROTOCOL_VERSION } from "./proto";
import type { Connection } from "./proto";

/** The About app: the facts about this farglass shell and its connection. */
export function About(props: { connection: Connection }) {
  return (
    <section class="about">
      <h1>farglass</h1>
      <p>The desktop of {window.location.hostname} in your browser.</p>
      <dl>
        <dt>User</dt>
        <dd>{props.connection.user ?? "none"}</dd>
        <dt>Host</dt>
        <dd>{window.location.host}</dd>
        <dt>Channel protocol</dt>
        <dd>Version {PROTOCOL_VERSION}</dd>
      </dl>
    </section>
  );
}
