#!/bin/sh
# drive.sh <cmd> [args]: input for the private headless display.
#   drive.sh keys <wtype args…>   one wtype run, e.g. keys hello -k Tab world
#   drive.sh move <x> <y> | click <x> <y>
# Pointer coordinates are logical pixels of the headless output (1200x750).
# Ghostty takes a left click on an unfocused surface as a focus click and does
# not report it, and the display has a keyboard (so a focused window) only
# while wtype runs: `click` holds a virtual keyboard around the click.
# A fresh virtual keyboard drops its first key, so each keys run starts with
# a short sleep; a real pointer comes from scripts/vpointer.py.
set -eu
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/hotty"
export WAYLAND_DISPLAY="$(cat "$CACHE/sway/display")"
# The display headless.sh started, not a stale one that sorts first.
SOCK="$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$(cat "$CACHE/sway/pid" 2>/dev/null).sock"
[ -S "$SOCK" ] || SOCK="$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock | head -1)"
swaymsg() { LD_LIBRARY_PATH="$CACHE/sway/root/usr/lib" "$CACHE/sway/root/usr/bin/swaymsg" -s "$SOCK" "$@" >/dev/null; }
wtype() {
  if [ ! -x "$CACHE/wtype/root/usr/bin/wtype" ]; then
    mkdir -p "$CACHE/wtype/pkgs" "$CACHE/wtype/root"
    for u in $(pacman -Sp wtype 2>/dev/null); do
      case "$u" in file://*) cp "${u#file://}" "$CACHE/wtype/pkgs/";; http*) (cd "$CACHE/wtype/pkgs" && curl -sSfLO "$u");; esac
    done
    for f in "$CACHE"/wtype/pkgs/*.pkg.tar.zst; do tar --zstd -C "$CACHE/wtype/root" -xf "$f"; done
  fi
  "$CACHE/wtype/root/usr/bin/wtype" "$@"
}
case "$1" in
  keys) shift; wtype -s 300 "$@" ;;
  move) python3 "$(dirname "$0")/vpointer.py" 1200 750 move "$2" "$3" ;;
  click)
    wtype -s 1500 & kb=$!
    sleep 0.4
    python3 "$(dirname "$0")/vpointer.py" 1200 750 move "$2" "$3" sleep 0.1 click
    wait $kb ;;
esac
