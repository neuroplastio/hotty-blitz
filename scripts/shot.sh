#!/bin/sh
# shot.sh <out.png> <seconds> <ghostty|kitty|path-to-ghostty> -- cmd...
# Runs cmd in a terminal on the private headless display and screenshots it.
set -eu
out=$1; wait=$2; term=$3; shift 3; [ "${1:-}" = "--" ] && shift
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/hotty/sway"
export WAYLAND_DISPLAY="$(cat "$CACHE/display")"
unset DISPLAY HYPRLAND_INSTANCE_SIGNATURE
case $term in
  kitty) kitty -o linux_display_server=wayland -e "$@" >/dev/null 2>&1 & ;;
  *ghostty*) "$( [ "$term" = ghostty ] && echo ghostty || echo "$term")" --gtk-single-instance=false --window-decoration=false -e "$@" >/dev/null 2>&1 & ;;
esac
pid=$!
sleep "$wait"
grim "$out"
kill $pid 2>/dev/null || true
wait $pid 2>/dev/null || true
