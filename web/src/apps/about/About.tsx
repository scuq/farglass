import { createSignal, onCleanup } from "solid-js";
import { DbusClient } from "../../dbus";
import { PROTOCOL_VERSION } from "../../proto";
import type { AppContext } from "../types";

/** The About app: the facts about this farglass shell and its connection. */
export default function About(props: { context: AppContext }) {
  const [system, setSystem] = createSignal<{ os: string; kernel: string } | null>(null);
  // The system of the host, from hostname1 on the system bus.
  const dbus = new DbusClient(props.context.connection, "system");
  dbus
    .getAllProperties("org.freedesktop.hostname1", "/org/freedesktop/hostname1", "org.freedesktop.hostname1")
    .then(
      (properties) =>
        setSystem({
          os: String(properties.OperatingSystemPrettyName ?? "unknown"),
          kernel: `${properties.KernelName ?? ""} ${properties.KernelRelease ?? ""}`.trim() || "unknown",
        }),
      () => setSystem({ os: "not available", kernel: "not available" }),
    );
  onCleanup(() => dbus.close());

  return (
    <section class="about">
      <h1>farglass</h1>
      <p>The desktop of {window.location.hostname} in your browser.</p>
      <dl>
        <dt>User</dt>
        <dd>{props.context.user ?? "none"}</dd>
        <dt>Host</dt>
        <dd>{window.location.host}</dd>
        <dt>Operating system</dt>
        <dd data-field="os">{system()?.os ?? "loading"}</dd>
        <dt>Kernel</dt>
        <dd data-field="kernel">{system()?.kernel ?? "loading"}</dd>
        <dt>Channel protocol</dt>
        <dd>Version {PROTOCOL_VERSION}</dd>
        <dt>Transport</dt>
        <dd data-field="transport">{props.context.connection.transport === "webtransport" ? "WebTransport (HTTP/3)" : "WebSocket"}</dd>
      </dl>
    </section>
  );
}
