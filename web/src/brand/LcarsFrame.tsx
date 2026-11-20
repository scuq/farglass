import type { JSX } from "@solidjs/web";
import logo from "./farglass.svg";

/** The URL of the farglass icon. It is also the image of a window or an app with no icon. */
export { logo };

/**
 * The LCARS frame of the screens before the desktop: the login, lock, and PIN screens.
 *
 * The bars are decoration only and screen readers skip them. The brand line is
 * not a heading, thus the heading of each screen stays the first `h1`.
 */
export function LcarsFrame(props: { children: JSX.Element }) {
  return (
    <div class="lcars-page">
      <div class="lcars">
        <div class="lcars-top" aria-hidden="true">
          <span />
          <span />
          <span />
          <span />
        </div>
        <div class="lcars-body">
          <div class="lcars-side" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <div class="lcars-content">
            <p class="lcars-brand">
              <img src={logo} alt="" width="32" height="32" />
              <span>farglass</span>
            </p>
            <p class="lcars-subtitle">Remote access system</p>
            {props.children}
            <div class="lcars-status">
              <span>{window.location.hostname}</span>
              <span>FG-01</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
