#!/bin/sh
# portal.sh - run a command with a private D-Bus session and its own desktop portal (F115)
#
# Usage: portal.sh farglass|none COMMAND...
#
# With "farglass", the portal uses the GTK backend, as in a web session
# (farglass-portals.conf), but no keyring: the private bus cannot start the
# keyring, and the portal waits 25 s for it. With "none", the portal finds no
# backend. GSettings stays in memory, so the camera lockdown of the user
# (org.gnome.desktop.privacy disable-camera) does not apply, and the script
# changes no setting of the user. With GRANT=yes, host apps get the camera
# with no Access dialog.
#
# The script runs in a session of its own. When COMMAND ends, it stops all
# other processes of that session: the services that the private bus started
# stay after the bus, else. To stop the script from a test, send the signal to
# its process group.
#
# Exit status: the status of COMMAND.

if [ -z "${PORTAL_INNER:-}" ]; then
  desktop=$1
  shift
  export GSETTINGS_BACKEND=memory XDG_CURRENT_DESKTOP="$desktop"
  # A private permission store and portal configuration, so the script does not change the ones of the user.
  XDG_DATA_HOME=$(mktemp -d)
  XDG_CONFIG_HOME=$(mktemp -d)
  export XDG_DATA_HOME XDG_CONFIG_HOME
  mkdir -p "$XDG_CONFIG_HOME/xdg-desktop-portal"
  printf '[preferred]\ndefault=gtk\n' >"$XDG_CONFIG_HOME/xdg-desktop-portal/farglass-portals.conf"
  if [ "$desktop" = none ]; then
    XDG_DESKTOP_PORTAL_DIR=$(mktemp -d)
    export XDG_DESKTOP_PORTAL_DIR
  fi
  # A new session: "kill 0" stops only the processes of this script. This shell ignores that TERM.
  exec setsid --wait sh -c '
    PORTAL_INNER=1 dbus-run-session -- "$0" "$@"
    status=$?
    trap "" TERM
    kill 0 2>/dev/null
    exit $status
  ' "$0" "$@"
fi

# The inner part: in the private D-Bus session.
/usr/libexec/xdg-desktop-portal -r >"${PORTAL_LOG:-/dev/null}" 2>&1 &
# The portal starts slowly on a private bus. Wait for it, at most 60 s.
i=0
until busctl --user get-property org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.Camera IsCameraPresent >/dev/null 2>&1 || [ $i -ge 60 ]; do
  sleep 1
  i=$((i + 1))
done
echo "portal: camera interface after ${i} s:" \
  "$(busctl --user get-property org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.Camera IsCameraPresent 2>&1)"
# With GRANT=yes, the permission store lets the host apps (app id "") use
# the camera, so the portal shows no Access dialog.
if [ "${GRANT:-}" = yes ]; then
  busctl --user call org.freedesktop.impl.portal.PermissionStore \
    /org/freedesktop/impl/portal/PermissionStore org.freedesktop.impl.portal.PermissionStore \
    SetPermission sbssas devices true camera "" 1 yes && echo "portal: camera granted to host apps"
fi
"$@"
