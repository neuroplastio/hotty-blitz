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
  before=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' || true)
  (env -u WAYLAND_DISPLAY -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_LIBINPUT_NO_DEVICES=1 \
    LD_LIBRARY_PATH="$CACHE/root/usr/lib" \
    setsid "$CACHE/root/usr/bin/sway" -c "$CACHE/config" >"$CACHE/sway.log" 2>&1 &)
  sleep 1.5
  after=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' || true)
  new=$(printf '%s\n%s\n' "$before" "$after" | sort | uniq -u | head -1)
  echo "${new:-?}" > "$CACHE/display"
  echo "headless sway on ${new:-?}"
  ;;
stop)
  pkill -x sway || true
  ;;
esac
