#!/bin/sh
# release.sh - build the farglass release files for one architecture (F066)
#
# Usage:
#   packaging/release.sh host VERSION OUTDIR   build the host programs and the tarball
#   packaging/release.sh ctl VERSION OUTDIR    build the static farglassctl
#
# host builds farglassd, farglass-login, and farglass-session with libstdc++
# static, checks that they link only the libraries of ADR-048, and writes
# OUTDIR/farglass-VERSION-linux-ARCH.tar.gz with the programs, the web files
# of web/dist, the extension, the systemd units, the PAM files, the portal
# file, and the default configuration. web/dist must exist.
#
# ctl builds farglassctl for the musl target of this machine, checks that it is
# static, and writes OUTDIR/farglassctl-linux-ARCH.
#
# Exit status: 0 on success, 1 on an error.

set -eu

die() {
    echo "release.sh: $*" >&2
    exit 1
}

[ $# -eq 3 ] || die "Usage: release.sh host|ctl VERSION OUTDIR"
MODE=$1
VERSION=$2
OUT=$3
REPO=$(cd "$(dirname "$0")/.." && pwd)
case "$(uname -m)" in
    x86_64) ARCH=amd64 TRIPLE=x86_64 ;;
    aarch64) ARCH=arm64 TRIPLE=aarch64 ;;
    *) die "The architecture $(uname -m) is not supported." ;;
esac
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
export FARGLASS_VERSION="$VERSION"

# The dynamic libraries that a host program can link directly (ADR-048, ADR-053).
ALLOWED='linux-vdso.so.1 ld-linux-x86-64.so.2 ld-linux-aarch64.so.1 libc.so.6 libm.so.6 libgcc_s.so.1 libpam.so.0 libaudit.so.1 libcap-ng.so.0 libpipewire-0.3.so.0 libxkbcommon.so.0'

# check_libraries - fails if FILE has a NEEDED entry that ALLOWED does not name
#
# ldd is not used: it also shows the libraries that libpam of the build
# container loads, for example libeconf on Rocky Linux 9.
check_libraries() {
    libs=$(readelf -d "$1" | awk '/\(NEEDED\)/ { gsub(/[][]/, "", $NF); print $NF }')
    [ -n "$libs" ] || die "readelf shows no library for $1."
    for lib in $libs; do
        case " $ALLOWED " in
            *" $lib "*) ;;
            *) die "$1 links $lib, which ADR-048 does not permit." ;;
        esac
    done
    echo "release.sh: $(basename "$1") links only permitted libraries"
}

case "$MODE" in
    host)
        [ -f "$REPO/web/dist/index.html" ] || die "web/dist is missing. Build it first: npm --prefix web run build"
        # libstdc++ of openh264 goes into the programs (ADR-048).
        stdcxx=$(dirname "$(c++ -print-file-name=libstdc++.a)")
        [ -f "$stdcxx/libstdc++.a" ] || die "libstdc++.a is missing. Install the static libstdc++ of the compiler."
        CXXSTDLIB=static=stdc++ RUSTFLAGS="-C link-arg=-static-libgcc -L native=$stdcxx" \
            cargo build --release --locked -p farglassd -p farglass-login -p farglass-session \
            --manifest-path "$REPO/Cargo.toml"
        name="farglass-$VERSION-linux-$ARCH"
        stage=$(mktemp -d)
        trap 'rm -rf "$stage"' EXIT
        root="$stage/$name"
        mkdir -p "$root/bin" "$root/web" "$root/extension" "$root/packaging"
        for program in farglassd farglass-login farglass-session; do
            check_libraries "$REPO/target/release/$program"
            install -m 0755 "$REPO/target/release/$program" "$root/bin/$program"
        done
        cp -R "$REPO/web/dist/." "$root/web/"
        cp -R "$REPO/extension/redirect" "$root/extension/redirect"
        cp -R "$REPO/packaging/systemd" "$REPO/packaging/pam.d" "$REPO/packaging/selinux" "$REPO/packaging/xdg-desktop-portal" "$root/packaging/"
        cp "$REPO/packaging/farglassd.conf" "$root/packaging/"
        echo "$VERSION" > "$root/VERSION"
        tar -C "$stage" --owner=0 --group=0 --numeric-owner -czf "$OUT/$name.tar.gz" "$name"
        echo "release.sh: wrote $OUT/$name.tar.gz"
        ;;
    ctl)
        target="$TRIPLE-unknown-linux-musl"
        # ring, the crypto of rustls, has C code: musl-gcc of musl-tools builds it for each architecture.
        command -v musl-gcc >/dev/null || die "musl-gcc is missing. Install musl-tools (Debian, Ubuntu) or musl-gcc (Fedora)."
        export "CC_${TRIPLE}_unknown_linux_musl=musl-gcc"
        cargo build --release --locked -p farglassctl --target "$target" --manifest-path "$REPO/Cargo.toml"
        bin="$REPO/target/$target/release/farglassctl"
        # file says "statically linked", or "static-pie linked" for the PIE build of musl. Both have no loader.
        file "$bin" | grep -Eq "statically linked|static-pie linked" || die "farglassctl is not static: $(file "$bin")"
        install -m 0755 "$bin" "$OUT/farglassctl-linux-$ARCH"
        echo "release.sh: wrote $OUT/farglassctl-linux-$ARCH (static)"
        ;;
    *)
        die "Usage: release.sh host|ctl VERSION OUTDIR"
        ;;
esac
