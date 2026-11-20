import { createSignal } from "solid-js";
import { DEFAULT_FOCUS, FOCUS_KEY, type FocusSettings, parseFocusSettings } from "../../../wm/focus";
import type { AppContext } from "../../types";

/** The rules of focus protection (F041). The value is in the state store, thus each tab and each login gets it. */
export function FocusPage(props: { context: AppContext }) {
  const [value, setValue] = createSignal<FocusSettings>(DEFAULT_FOCUS);
  props.context.state.watchShared(FOCUS_KEY, (stored) => setValue(parseFocusSettings(stored)));
  const change = (patch: Partial<FocusSettings>) => {
    const next = { ...value(), ...patch };
    setValue(next);
    props.context.state.setShared(FOCUS_KEY, next);
  };

  return (
    <div class="settings-page">
      <h2>Focus</h2>
      <p class="settings-note">The desktop does not interrupt you while you work.</p>
      <label class="settings-choice">
        <input type="checkbox" name="hold-notifications" checked={value().holdNotifications} onChange={(event) => change({ holdNotifications: event.currentTarget.checked })} />
        Notifications wait while you type or move the pointer, and show at a pause. Critical notifications show at once.
      </label>
      <label class="settings-choice">
        <input type="checkbox" name="quiet-windows" checked={value().quietWindows} onChange={(event) => change({ quietWindows: event.currentTarget.checked })} />
        A window that you did not ask for opens behind, with a mark in the panel. A window that you start, click for, or start with Enter gets the focus.
      </label>
    </div>
  );
}
