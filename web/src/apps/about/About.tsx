import { PROTOCOL_VERSION } from "../../proto";
import type { AppContext } from "../types";

/** The About app: the facts about this farglass shell and its connection. */
export default function About(props: { context: AppContext }) {
  return (
    <section class="about">
      <h1>farglass</h1>
      <p>The desktop of {window.location.hostname} in your browser.</p>
      <dl>
        <dt>User</dt>
        <dd>{props.context.user ?? "none"}</dd>
        <dt>Host</dt>
        <dd>{window.location.host}</dd>
        <dt>Channel protocol</dt>
        <dd>Version {PROTOCOL_VERSION}</dd>
      </dl>
    </section>
  );
}
