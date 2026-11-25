#!/usr/bin/env bash
# Ryoku Box — connection helper.
# Normal use needs NO command at all: open the app, it finds the PC over Wi-Fi.
# This script is only for (re)installing the APK after a token or network change.
set -euo pipefail
cd "$(dirname "$0")/.."
PORT=8875

case "${1:-status}" in
  setup)
    echo "== building APK with this PC's current token and LAN address =="
    ./android/build.sh >/dev/null
    D=$(adb devices | awk 'NR>1 && $2=="device" {print $1; exit}')
    [ -n "$D" ] || { echo "No phone connected via adb." >&2; exit 1; }
    echo "== installing on $D =="
    adb -s "$D" install -r android/RyokuBox.apk 2>&1 | tail -1
    adb -s "$D" shell am force-stop com.ryoku.ryokubox
    adb -s "$D" shell monkey -p com.ryoku.ryokubox -c android.intent.category.LAUNCHER 1 >/dev/null
    cp android/RyokuBox.apk "$HOME/Downloads/RyokuBox.apk"
    echo "Done. Open the app — the deck appears with no input."
    ;;
  status)
    IP="$(ip -4 -br addr show 2>/dev/null | awk '$3 !~ /^127\./ {sub(/\/.*/,"",$3); print $3; exit}')"
    echo "agent:      $(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$PORT/) on http://$IP:$PORT/"
    echo "mdns:       $(systemctl --user is-active ryokubox-mdns.service 2>/dev/null || echo inactive)  (_ryokubox._tcp)"
    echo "firewall:   $(grep -c -- "--dport $PORT -j ACCEPT" /etc/ufw/user.rules 2>/dev/null || echo 0) rule(s) for $PORT"
    echo "phones:"; adb devices | tail -n +2
    echo
    echo "Normal use: open the app. No cable, no code, no debugging."
    echo "After a token/network change: $0 setup"
    ;;
  *)
    echo "usage: $0 {setup|status}" >&2
    exit 2
    ;;
esac
