import { describe, expect, test } from "vitest";
import { MEDIA_LABEL, mediaSummary } from "./media";

describe("the summary of the media menu", () => {
  test("with no exception, the button shows Media", () => {
    expect(mediaSummary({})).toEqual({ label: MEDIA_LABEL, live: false });
    expect(mediaSummary({ muted: false, mic: "off", camera: "off" })).toEqual({ label: "Media", live: false });
    expect(mediaSummary({ mic: "refused", camera: "blocked" })).toEqual({ label: "Media", live: false });
  });

  test("a live microphone or camera is named and live", () => {
    expect(mediaSummary({ mic: "on" })).toEqual({ label: "Mic on", live: true });
    expect(mediaSummary({ camera: "on" })).toEqual({ label: "Cam on", live: true });
    expect(mediaSummary({ muted: true, mic: "on", camera: "on" })).toEqual({ label: "Mic on · Cam on · Muted", live: true });
  });

  test("an armed camera and a muted sound are named, and are not live", () => {
    expect(mediaSummary({ camera: "armed" })).toEqual({ label: "Cam ready", live: false });
    expect(mediaSummary({ muted: true })).toEqual({ label: "Muted", live: false });
    expect(mediaSummary({ mic: "on", camera: "armed" })).toEqual({ label: "Mic on · Cam ready", live: true });
  });
});
