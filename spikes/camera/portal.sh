#!/bin/sh
# portal.sh - run a command with a private D-Bus session and its own desktop portal (F115)
#
# Usage: portal.sh farglass|none COMMAND...
#
# With "farglass", the portal uses farglass-portals.conf, as in a web session:
# the GTK backend. With "none", the portal finds no backend.
# GSettings stays in memory, so the camera lockdown of the user
# (org.gnome.desktop.privacy disable-camera) does not apply, and the script
# changes no setting of the user. With GRANT=yes, host apps get the camera with
# no Access dialog.
#
# Exit status: the status of COMMAND.

desktop=$1
shift
export GSETTINGS_BACKEND=memory XDG_CURRENT_DESKTOP="$desktop"
# A private permission store, so the script does not change the one of the user.
XDG_DATA_HOME=$(mktemp -d)
export XDG_DATA_HOME
if [ "$desktop" = none ]; then
  XDG_DESKTOP_PORTAL_DIR=$(mktemp -d)
  export XDG_DESKTOP_PORTAL_DIR
fi
exec dbus-run-session -- sh -c '
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
' portal "$@"
