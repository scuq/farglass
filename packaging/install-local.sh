#!/bin/sh
# install-local.sh - install farglass from this source tree on the local host
#
# Usage:
#   sudo packaging/install-local.sh install [ADDRESS...]
#   sudo packaging/install-local.sh uninstall [--purge]
#
# Build first as your own user:
#   cargo build --release -p farglassd -p farglass-login -p farglass-session
#   npm --prefix web run build
#
# install copies the programs, the shell files, the PAM files, and the systemd
# units, creates the system user farglass, and starts farglass-login.socket and
# farglassd.service. farglassd listens on 0.0.0.0:8443 with a self-signed
# certificate. The certificate has the hostname, localhost, the address of the
# default route, and each ADDRESS.
#
# install does not change an existing /etc/farglass/farglassd.conf.
# uninstall removes all files except the configuration, the state, and the
# user farglass. --purge also removes them.
#
# The Debian package (F033) replaces this script.
#
# Exit status: 0 on success, 1 on an error.

set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
LIBEXEC=/usr/libexec/farglass
WEB=/usr/share/farglass/web
CONF=/etc/farglass/farglassd.conf
UNITS="farglass-login.socket farglass-login.service farglassd.service"
PORT=8443

die() {
    echo "install-local.sh: $*" >&2
    exit 1
}

[ "$(id -u)" -eq 0 ] || die "Run this script with sudo."

install_all() {
    for program in farglassd farglass-login farglass-session; do
        [ -x "$REPO/target/release/$program" ] ||
            die "$REPO/target/release/$program is missing. Build it first: cargo build --release -p $program"
    done
    [ -f "$REPO/web/dist/index.html" ] || die "The shell is not built. Build it first: npm --prefix web run build"

    getent group farglass >/dev/null || groupadd --system farglass
    id farglass >/dev/null 2>&1 ||
        useradd --system --gid farglass --home-dir /var/lib/farglass --no-create-home \
            --shell /usr/sbin/nologin --comment "farglass web server" farglass

    for program in farglassd farglass-login farglass-session; do
        install -D -m 0755 "$REPO/target/release/$program" "$LIBEXEC/$program"
    done
    rm -rf "$WEB"
    mkdir -p "$WEB"
    cp -R "$REPO/web/dist/." "$WEB/"
    chmod -R u=rwX,go=rX "$WEB"

    install -D -m 0644 "$REPO/packaging/pam.d/farglass" /etc/pam.d/farglass
    install -D -m 0644 "$REPO/packaging/pam.d/farglass-session" /etc/pam.d/farglass-session

    if [ -e "$CONF" ]; then
        echo "Keeping the existing $CONF."
    else
        names=$(ip -4 route get 192.0.2.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p')
        for address in "$@"; do
            names="$names $address"
        done
        list=$(printf '%s\n' $names | sed '/^$/d; s/.*/"&"/' | paste -sd, | sed 's/,/, /g')
        mkdir -p /etc/farglass
        sed -e "s|^#listen = .*|listen = \"0.0.0.0:$PORT\"|" \
            -e "s|^#names = .*|names = [$list]|" \
            "$REPO/packaging/farglassd.conf" > "$CONF"
        chmod 0644 "$CONF"
        echo "Wrote $CONF: listen 0.0.0.0:$PORT, certificate names [$list]."
    fi

    for unit in $UNITS; do
        install -D -m 0644 "$REPO/packaging/systemd/$unit" "/etc/systemd/system/$unit"
    done
    systemctl daemon-reload
    systemctl enable --now farglass-login.socket
    # A running farglass-login keeps the old program. systemd starts the new one at the next login.
    systemctl stop farglass-login.service
    systemctl enable farglassd.service
    systemctl restart farglassd.service

    i=0
    until curl -ks -o /dev/null "https://127.0.0.1:$PORT/"; do
        i=$((i + 1))
        if [ "$i" -ge 20 ]; then
            journalctl -u farglassd -u farglass-login --no-pager -n 30
            die "farglassd does not answer on port $PORT. The log is above."
        fi
        sleep 0.5
    done
    echo
    echo "farglass runs. Open one of these addresses in a browser:"
    for address in $(ip -4 -o addr show scope global | awk '{print $4}' | cut -d/ -f1); do
        echo "  https://$address:$PORT/"
    done
    echo
    echo "Before you accept the self-signed certificate, compare its SHA-256 fingerprint:"
    journalctl -u farglassd --no-pager -o cat | sed 's/\x1b\[[0-9;]*m//g' | grep -o 'sha256=[0-9A-F:]*' | tail -1

    # The web sessions continue through an update, and their agents keep the old program.
    sessions=$(loginctl --json=short list-sessions | python3 -c '
import json, subprocess, sys
for s in json.load(sys.stdin):
    if s["class"] != "user":
        continue
    out = subprocess.run(["loginctl", "show-session", s["session"], "-p", "Type", "-p", "Service"],
                         capture_output=True, text=True).stdout
    if "Type=web" in out and "Service=farglass-session" in out:
        print("  session %s of %s" % (s["session"], s["user"]))
')
    if [ -n "$sessions" ]; then
        echo
        echo "These web sessions still run the old agent. Log out in the shell and log in again,"
        echo "or end them with: loginctl terminate-session ID"
        echo "$sessions"
    fi
}

uninstall_all() {
    for unit in farglassd.service farglass-login.socket farglass-login.service; do
        systemctl disable --now "$unit" 2>/dev/null || true
    done
    for unit in $UNITS; do
        rm -f "/etc/systemd/system/$unit"
    done
    systemctl daemon-reload
    rm -rf "$LIBEXEC" /usr/share/farglass /etc/pam.d/farglass /etc/pam.d/farglass-session /run/farglass
    if [ "${1:-}" = "--purge" ]; then
        rm -rf /etc/farglass /var/lib/farglass
        userdel farglass 2>/dev/null || true
        groupdel farglass 2>/dev/null || true
        echo "Removed farglass, its configuration, its state, and the user farglass."
    else
        echo "Removed farglass. The configuration in /etc/farglass, the state in /var/lib/farglass, and the user farglass stay."
    fi
}

case "${1:-}" in
    install)
        shift
        install_all "$@"
        ;;
    uninstall)
        shift
        uninstall_all "$@"
        ;;
    *)
        die "Usage: sudo $0 install [ADDRESS...] | uninstall [--purge]"
        ;;
esac
