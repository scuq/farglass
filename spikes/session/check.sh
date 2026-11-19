#!/bin/sh
# check.sh - test the real web session with root
#
# Usage: sudo spikes/session/check.sh
#
# Build first as your own user:
#   cargo build -p farglassd -p farglass-login -p farglass-session
#   npm --prefix web run build
#
# The script does these tests:
#   1. It creates the temporary user farglass-test, which has no local session.
#      A login through the shell must give a logind session of type web, with
#      the agent as farglass-test. A second login must attach to the same session.
#      pkexec in a terminal must show the polkit dialog, and Cancel must end it.
#      notify-send in a terminal must show a notification, and its action must go back.
#      The Log out button must end the web session and the agent.
#   2. If the user who runs sudo has a local graphical session, a login as that
#      user must show the confirm step. The script clicks Cancel only, and the
#      local session must continue.
# PAM uses a test stack: pam_exec accepts the password "secret" for each user.
# The script removes farglass-test and all its files at the end.
#
# Exit status: 0 if all tests pass, 1 if a test fails.

set -eu

TEST_USER=farglass-test
REPO=$(cd "$(dirname "$0")/../.." && pwd)
FAILED=0

fail() { echo "FAIL: $*"; FAILED=1; }
pass() { echo "PASS: $*"; }

if [ "$(id -u)" -ne 0 ] || [ -z "${SUDO_USER:-}" ]; then
    echo "check.sh: Run this script with sudo from your own user." >&2
    exit 1
fi
for program in farglassd farglass-login farglass-session; do
    if [ ! -x "$REPO/target/debug/$program" ]; then
        echo "check.sh: $REPO/target/debug/$program is missing. Build it first as $SUDO_USER." >&2
        exit 1
    fi
done
if id "$TEST_USER" >/dev/null 2>&1; then
    echo "check.sh: The user $TEST_USER exists. Remove it first: userdel -r $TEST_USER" >&2
    exit 1
fi

# /run is mounted noexec. The root filesystem lets the copied programs run.
DIR=$(mktemp -d /var/lib/farglass-check.XXXXXX)
chmod 0755 "$DIR"
PIDS=""

cleanup() {
    for pid in $PIDS; do kill "$pid" 2>/dev/null || true; done
    loginctl terminate-user "$TEST_USER" 2>/dev/null || true
    sleep 1
    systemctl stop "farglass-session-$(id -u "$TEST_USER" 2>/dev/null || echo 0).service" 2>/dev/null || true
    userdel -r "$TEST_USER" 2>/dev/null || true
    rm -rf "$DIR"
}
trap cleanup EXIT INT TERM

# The programs must be in a directory that the test user can read.
mkdir -p "$DIR/bin" "$DIR/run" "$DIR/pam.d" "$DIR/state"
cp "$REPO/target/debug/farglassd" "$REPO/target/debug/farglass-login" "$REPO/target/debug/farglass-session" "$DIR/bin/"
chown "$SUDO_USER" "$DIR/run" "$DIR/state"

printf '#!/bin/sh\nIFS= read -r password; [ "$password" = secret ]\n' > "$DIR/check-password.sh"
chmod 0755 "$DIR/check-password.sh"
cat > "$DIR/pam.d/farglass" <<PAM
auth required pam_exec.so expose_authtok quiet $DIR/check-password.sh
account required pam_permit.so
PAM
cp "$REPO/packaging/pam.d/farglass-session" "$DIR/pam.d/farglass-session"
# With --pam-dir, PAM finds the files of @include in the same directory.
cp /etc/pam.d/common-* "$DIR/pam.d/"

useradd --create-home --shell /bin/sh "$TEST_USER"
TEST_UID=$(id -u "$TEST_USER")

"$DIR/bin/farglass-login" --socket "$DIR/run/login.sock" --socket-mode 666 \
    --pam-dir "$DIR/pam.d" --fail-delay-ms 0 \
    --agent-program "$DIR/bin/farglass-session" --agent-socket "$DIR/run/agent.sock" \
    2> "$DIR/login.log" &
PIDS="$PIDS $!"
runuser -u "$SUDO_USER" -- env NO_COLOR=1 "$DIR/bin/farglassd" --listen 127.0.0.1:18447 \
    --web-root "$REPO/web/dist" --state-dir "$DIR/state" \
    --agent-socket "$DIR/run/agent.sock" --login-socket "$DIR/run/login.sock" \
    2> "$DIR/farglassd.log" &
PIDS="$PIDS $!"
sleep 2
for pid in $PIDS; do
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "check.sh: A program stopped at the start. The logs:" >&2
        cat "$DIR/login.log" "$DIR/farglassd.log" >&2
        exit 1
    fi
done
URL=https://localhost:18447

USER_HOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
browser() {
    # runuser keeps the environment of root. Playwright finds its browser in the home of the user.
    runuser -u "$SUDO_USER" -- env HOME="$USER_HOME" node "$REPO/spikes/e2e/session-browser.mjs" "$URL" "$1" "$2"
}

web_sessions() {
    for id in $(loginctl --json=short list-sessions | python3 -c "
import json, sys
for s in json.load(sys.stdin):
    if s['uid'] == $1 and s['class'] == 'user':
        print(s['session'])"); do
        if loginctl show-session "$id" -p Type -p Service | grep -q '^Type=web$' &&
           loginctl show-session "$id" -p Service | grep -q '^Service=farglass-session$'; then
            echo "$id"
        fi
    done
}

echo "== 1. A user with no local session: $TEST_USER"
if browser "$TEST_USER" success; then
    sessions=$(web_sessions "$TEST_UID")
    count=$(echo "$sessions" | grep -c . || true)
    if [ "$count" -eq 1 ]; then
        pass "loginctl shows one session of type web: $(loginctl show-session $sessions -p Id -p Type -p Class -p Service -p State | tr '\n' ' ')"
    else
        fail "loginctl shows $count web sessions of $TEST_USER"
    fi
    if pgrep -u "$TEST_USER" -f "$DIR/bin/farglass-session" >/dev/null; then
        pass "the agent runs as $TEST_USER"
    else
        fail "no agent runs as $TEST_USER"
    fi
    if browser "$TEST_USER" success && [ "$(web_sessions "$TEST_UID")" = "$sessions" ]; then
        pass "a second login attaches to the same session $sessions"
    else
        fail "the second login did not attach to session $sessions"
    fi
    echo "INFO: logind: $(loginctl show-user "$TEST_USER" -p Display -p Sessions | tr '\n' ' ')"
    if browser "$TEST_USER" polkit; then
        pass "the polkit agent of the web session shows the prompt of pkexec in the shell"
    else
        fail "the polkit prompt of pkexec did not work"
        journalctl _UID="$TEST_UID" --since "-2 min" --no-pager -o cat 2>/dev/null | grep -i polkit | tail -5
        journalctl -u polkit --since "-2 min" --no-pager -o cat | tail -5
    fi
    if browser "$TEST_USER" notify; then
        pass "the agent is the notification service of the web session"
    else
        fail "the notification of notify-send did not work"
        journalctl _UID="$TEST_UID" --since "-2 min" --no-pager -o cat 2>/dev/null | grep -i notif | tail -5
    fi
    if browser "$TEST_USER" logout; then
        gone=no
        for _ in $(seq 50); do
            if [ -z "$(web_sessions "$TEST_UID")" ]; then gone=yes; break; fi
            sleep 0.3
        done
        if [ "$gone" = yes ]; then
            pass "the logout ended the web session of $TEST_USER"
        else
            fail "the web session of $TEST_USER continues after the logout"
        fi
        if pgrep -u "$TEST_USER" -f "$DIR/bin/farglass-session" >/dev/null; then
            fail "the agent of $TEST_USER runs after the logout"
        else
            pass "the agent of $TEST_USER stopped"
        fi
    else
        fail "the logout of $TEST_USER failed"
    fi
else
    fail "the login of $TEST_USER failed"
fi

echo "== 2. The user $SUDO_USER, if it has a local graphical session"
local_id=""
for id in $(loginctl --json=short list-sessions | python3 -c "
import json, sys
for s in json.load(sys.stdin):
    if s['user'] == '$SUDO_USER' and s['class'] == 'user':
        print(s['session'])"); do
    if loginctl show-session "$id" -p Type | grep -Eq '^Type=(x11|wayland|mir)$'; then
        local_id=$id
    fi
done
if [ -n "$local_id" ]; then
    if browser "$SUDO_USER" decline; then
        if loginctl show-session "$local_id" -p State | grep -Eq '^State=(active|online)$'; then
            pass "the local session $local_id of $SUDO_USER continues"
        else
            fail "the local session $local_id of $SUDO_USER is not active"
        fi
        if [ -z "$(web_sessions "$(id -u "$SUDO_USER")")" ]; then
            pass "no web session of $SUDO_USER started"
        else
            fail "a web session of $SUDO_USER started after Cancel"
        fi
    else
        fail "the confirm step for $SUDO_USER did not work"
    fi
else
    echo "SKIP: $SUDO_USER has no local graphical session"
fi

if [ "$FAILED" -ne 0 ]; then
    echo "== The logs:"
    sed 's/\x1b\[[0-9;]*m//g' "$DIR/login.log" | tail -30
    sed 's/\x1b\[[0-9;]*m//g' "$DIR/farglassd.log" | tail -15
    journalctl -u "farglass-session-$TEST_UID" --no-pager -n 20 2>/dev/null || true
fi
exit "$FAILED"
