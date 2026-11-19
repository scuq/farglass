import { createSignal, For, onCleanup, Show } from "solid-js";
import type { DbusClient, Variant } from "../../../dbus";
import { Loading } from "../common";

const NM = "org.freedesktop.NetworkManager";
const NM_PATH = "/org/freedesktop/NetworkManager";

/** The names of the device types of NetworkManager that a host usually has. */
const DEVICE_TYPES: Record<number, string> = {
  1: "Ethernet",
  2: "Wi-Fi",
  5: "Bluetooth",
  8: "Mobile broadband",
  10: "Bond",
  11: "VLAN",
  13: "Bridge",
  14: "Generic",
  15: "Team",
  16: "TUN",
  20: "Virtual Ethernet",
  22: "Dummy",
  29: "WireGuard",
  32: "Loopback",
};

/** The states of a device. "activated" is the usual state of a device in use. */
const DEVICE_STATES: Record<number, string> = {
  10: "Not managed",
  20: "Not available",
  30: "Disconnected",
  40: "Connecting",
  50: "Connecting",
  60: "Needs authentication",
  70: "Getting an address",
  80: "Checking",
  90: "Connecting",
  100: "Connected",
  110: "Disconnecting",
  120: "Failed",
};

/** The overall states of NetworkManager. */
const NM_STATES: Record<number, string> = {
  10: "Asleep",
  20: "Disconnected",
  30: "Disconnecting",
  40: "Connecting",
  50: "Connected to the local network only",
  60: "Connected to a site network only",
  70: "Connected",
};

export interface Device {
  readonly path: string;
  readonly name: string;
  readonly type: string;
  readonly state: string;
  readonly level: "ok" | "warning" | "off";
  readonly connection: string | null;
  readonly addresses: readonly string[];
  readonly gateway: string | null;
  readonly hardware: string | null;
}

/** Returns the addresses of an IP4Config or IP6Config object, as "address/prefix". */
export function addresses(data: unknown): string[] {
  if (!Array.isArray(data)) {
    return [];
  }
  return data.flatMap((entry) => {
    const record = entry as Record<string, Variant>;
    const address = record.address?.value;
    const prefix = record.prefix?.value;
    return typeof address === "string" ? [prefix === undefined ? address : `${address}/${String(prefix)}`] : [];
  });
}

const NO_OBJECT = "/";

/** The network devices of the host, from NetworkManager. The page only shows: a change of the network can stop this connection. */
export function NetworkPage(props: { dbus: DbusClient }) {
  const [devices, setDevices] = createSignal<readonly Device[] | null>(null);
  const [overall, setOverall] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);

  const device = async (path: string): Promise<Device> => {
    const d = await props.dbus.getAllProperties(NM, path, `${NM}.Device`);
    const ip = async (object: unknown, iface: string) => {
      if (typeof object !== "string" || object === NO_OBJECT) {
        return { list: [] as string[], gateway: null as string | null };
      }
      const config = await props.dbus.getAllProperties(NM, object, `${NM}.${iface}`).catch(() => ({}) as Record<string, unknown>);
      return { list: addresses(config.AddressData), gateway: typeof config.Gateway === "string" && config.Gateway !== "" ? config.Gateway : null };
    };
    const [v4, v6] = await Promise.all([ip(d.Ip4Config, "IP4Config"), ip(d.Ip6Config, "IP6Config")]);
    let connection: string | null = null;
    if (typeof d.ActiveConnection === "string" && d.ActiveConnection !== NO_OBJECT) {
      connection = await props.dbus
        .getProperty(NM, d.ActiveConnection, `${NM}.Connection.Active`, "Id")
        .then((id) => String(id))
        .catch(() => null);
    }
    const state = Number(d.State ?? 0);
    return {
      path,
      name: String(d.Interface ?? path),
      type: DEVICE_TYPES[Number(d.DeviceType)] ?? "Other",
      state: DEVICE_STATES[state] ?? "Unknown",
      level: state === 100 ? "ok" : state === 120 || state === 60 ? "warning" : "off",
      connection,
      addresses: [...v4.list, ...v6.list],
      gateway: v4.gateway ?? v6.gateway,
      hardware: typeof d.HwAddress === "string" && d.HwAddress !== "" ? d.HwAddress : null,
    };
  };

  const load = async () => {
    try {
      const main = await props.dbus.getAllProperties(NM, NM_PATH, NM);
      setOverall(NM_STATES[Number(main.State)] ?? "Unknown");
      const paths = Array.isArray(main.Devices) ? (main.Devices as string[]) : [];
      const list = await Promise.all(paths.map(device));
      setDevices(list.filter((each) => each.type !== "Loopback"));
    } catch {
      setError("NetworkManager does not run on this host.");
    }
  };
  void load();

  // NetworkManager sends many signals during a change. One load follows a quiet time.
  let timer: ReturnType<typeof setTimeout> | undefined;
  const later = () => {
    clearTimeout(timer);
    timer = setTimeout(() => void load(), 300);
  };
  const stops: (() => void)[] = [];
  let closed = false;
  for (const iface of [NM, `${NM}.Device`]) {
    props.dbus.subscribe({ sender: NM, interface: iface, member: "StateChanged" }, later).then(
      (stop) => (closed ? stop() : stops.push(stop)),
      () => {},
    );
  }
  onCleanup(() => {
    closed = true;
    clearTimeout(timer);
    for (const stop of stops) stop();
  });

  /** Reports whether the browser reaches the host through an address of `each`. */
  const carriesThisConnection = (each: Device) => each.addresses.some((address) => address.split("/")[0] === window.location.hostname.replace(/^\[|\]$/g, ""));

  return (
    <div class="settings-page">
      <h2>Network</h2>
      <Loading when={devices()} error={error()}>
        {(list) => (
          <>
            <p data-field="network-state">{overall()}</p>
            <ul class="settings-list">
              <For each={list()}>
                {(each) => (
                  <li class="settings-item">
                    <div class="settings-item-head">
                      <strong>{each.name}</strong>
                      <span class="settings-muted">{each.type}</span>
                      <span class="settings-state" data-level={each.level}>
                        {each.state}
                      </span>
                    </div>
                    <Show when={carriesThisConnection(each)}>
                      <p class="settings-note">This connection to farglass uses this device.</p>
                    </Show>
                    <dl class="settings-facts">
                      <Show when={each.connection}>
                        <dt>Connection</dt>
                        <dd>{each.connection}</dd>
                      </Show>
                      <Show when={each.addresses.length > 0}>
                        <dt>Addresses</dt>
                        <dd>{each.addresses.join(", ")}</dd>
                      </Show>
                      <Show when={each.gateway}>
                        <dt>Gateway</dt>
                        <dd>{each.gateway}</dd>
                      </Show>
                      <Show when={each.hardware}>
                        <dt>Hardware address</dt>
                        <dd>{each.hardware}</dd>
                      </Show>
                    </dl>
                  </li>
                )}
              </For>
            </ul>
            <p class="settings-note">
              The page shows the network and does not change it: a change can stop the connection of this desktop. Use nmcli in a
              terminal to change it.
            </p>
          </>
        )}
      </Loading>
    </div>
  );
}
