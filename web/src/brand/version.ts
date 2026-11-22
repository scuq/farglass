/**
 * Returns the version code of the LCARS frame: "FG-" and two digits for each part of the version.
 * A suffix such as "-rc1" is not part of the code. For example, "0.1.4" gives "FG-000104".
 */
export function versionCode(version: string): string {
  const parts = (version.split(/[-+]/)[0] ?? "").split(".").slice(0, 3);
  while (parts.length < 3) parts.push("0");
  return `FG-${parts.map((part) => String(Number(part) || 0).padStart(2, "0")).join("")}`;
}

/** The version code of this build. */
export const VERSION_CODE = versionCode(__FARGLASS_VERSION__);
