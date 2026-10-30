#!/bin/sh
# A private headless Wayland display for screenshots of terminals.
#
#   scripts/headless.sh start        # fetch sway (no root) and start it
#   scripts/headless.sh stop
#   scripts/shot.sh out.png 5 ghostty -- cmd...
#
# Never use `hyprctl output create headless`: the maintainer's Hyprland config
# moves workspaces onto any new monitor.
set -eu
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/hotty/sway"
cmd="${1:-start}"
case "$cmd" in
start)
  if [ ! -x "$CACHE/root/usr/bin/sway" ]; then
    mkdir -p "$CACHE/pkgs" "$CACHE/root"
    for u in $(pacman -Sp sway wlroots0.20 2>/dev/null); do
      case "$u" in
        file://*) cp "${u#file://}" "$CACHE/pkgs/" ;;
        http*) (cd "$CACHE/pkgs" && curl -sSfLO "$u") ;;
      esac
    done
    for f in "$CACHE"/pkgs/*.pkg.tar.zst; do tar --zstd -C "$CACHE/root" -xf "$f"; done
  fi
  cat > "$CACHE/config" <<CFG
output HEADLESS-1 resolution 2400x1500 scale 2
default_border none
focus_follows_mouse no
CFG
  # Other sways may run beside this one (other sessions', the rig's), so it
  # is told apart by the display and the IPC socket (sway-ipc.<uid>.<pid>.sock)
  # that appear while it starts.
  sockets() { ls "$XDG_RUNTIME_DIR" | grep -E '^(wayland-[0-9]+|sway-ipc\.[0-9]+\.[0-9]+\.sock)$' || true; }
  before=$(sockets)
  (env -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_LIBINPUT_NO_DEVICES=1 \
    LD_LIBRARY_PATH="$CACHE/root/usr/lib" \
    setsid "$CACHE/root/usr/bin/sway" -c "$CACHE/config" >"$CACHE/sway.log" 2>&1 &)
  sleep 1.5
  new=$(sockets | grep -vxF -e "$before" || true)
  display=$(echo "$new" | grep -m1 '^wayland-' || true)
  echo "${display:-?}" > "$CACHE/display"
  echo "$new" | sed -n 's/^sway-ipc\.[0-9]*\.\([0-9]*\)\.sock$/\1/p' | head -1 > "$CACHE/pid"
  echo "headless sway on ${display:-?}"
  ;;
stop)
  # Only the sway `start` began, if it still runs on this config: never
  # another's.
  pid=$(cat "$CACHE/pid" 2>/dev/null || true)
  if [ -n "$pid" ] && [ "$(tr '\0' ' ' 2>/dev/null <"/proc/$pid/cmdline")" = "$CACHE/root/usr/bin/sway -c $CACHE/config " ]; then
    kill "$pid"
  else
    echo "headless.sh: no sway of its own to stop" >&2
  fi
  rm -f "$CACHE/pid"
  ;;
esac
