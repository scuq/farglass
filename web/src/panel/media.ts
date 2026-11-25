/**
 * The states and the labels of the media menu of the panel: sound, microphone, and camera (F114, F116, F119).
 *
 * This file is the one source of these labels: the menu items and the panel button read them here.
 * The panel button shows only the exceptions. A microphone or a camera that sends to the host is a
 * matter of privacy: the button names it and shows it highlighted, with no click.
 */

import type { MicState } from "../audio/mic";
import type { CameraState } from "../camera/camera";

/** The text and the tooltip of the camera item for each state. The host gives the tooltip of a block. */
export const CAMERA_ITEM: Record<CameraState, { label: string; title: string }> = {
  off: { label: "Cam off", title: "Let the apps of the host use the camera of this computer" },
  armed: { label: "Cam ready", title: "The camera turns on when an app of the host opens it. Click to turn it off." },
  on: { label: "Cam on", title: "An app of the host gets the pictures of this camera. Click to turn it off." },
  refused: { label: "Cam blocked", title: "The browser refused the camera. Allow it in the site settings of the browser, then click again." },
  blocked: { label: "Cam disabled", title: "" },
};

/** The text and the tooltip of the microphone item for each state. */
export const MIC_ITEM: Record<MicState, { label: string; title: string }> = {
  off: { label: "Mic off", title: "Let the apps of the host use the microphone of this computer" },
  on: { label: "Mic on", title: "The apps of the host get the sound of this microphone. Click to turn it off." },
  refused: { label: "Mic blocked", title: "The browser refused the microphone. Allow it in the site settings of the browser, then click again." },
};

/** The text of the sound item. */
export function soundLabel(muted: boolean): string {
  return muted ? "Sound off" : "Sound";
}

/** The states that the panel button summarizes. A control that the session does not have is undefined. */
export interface MediaStates {
  readonly muted?: boolean | undefined;
  readonly mic?: MicState | undefined;
  readonly camera?: CameraState | undefined;
}

/** The label of the panel button with no exception. */
export const MEDIA_LABEL = "Media";

/**
 * Returns the text of the panel button, and if it shows a live capture.
 * The button names each exception, in this order: Mic on, Cam on, Cam ready, Muted. With none, it shows Media.
 */
export function mediaSummary(states: MediaStates): { label: string; live: boolean } {
  const parts: string[] = [];
  if (states.mic === "on") parts.push(MIC_ITEM.on.label);
  if (states.camera === "on") parts.push(CAMERA_ITEM.on.label);
  if (states.camera === "armed") parts.push(CAMERA_ITEM.armed.label);
  if (states.muted) parts.push("Muted");
  return { label: parts.length ? parts.join(" · ") : MEDIA_LABEL, live: states.mic === "on" || states.camera === "on" };
}
