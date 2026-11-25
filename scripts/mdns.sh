#!/usr/bin/env bash
# Publish the agent over mDNS with its current IPv4 and a fresh one-time pairing
# code. The phone's built-in "scanner" reads both from the TXT record, so no
# camera and no baked secret are needed. The code rotates with the console code.
set -uo pipefail

IP="$(ip -4 -br addr show 2>/dev/null | awk '$3 !~ /^127\./ {sub(/\/.*/,"",$3); print $3; exit}')"
[ -n "$IP" ] || IP="127.0.0.1"

CHILD=""
cleanup() {
  if [ -n "$CHILD" ]; then kill "$CHILD" 2>/dev/null || true; wait "$CHILD" 2>/dev/null || true; fi
  CHILD=""
}
trap cleanup EXIT INT TERM

while true; do
  CODE="$(curl -fsS --max-time 3 http://127.0.0.1:8875/console 2>/dev/null \
    | grep -o 'pair=[0-9a-f]*' | head -1 | cut -d= -f2 || true)"
  [ -n "$CODE" ] || CODE="none"

  # Service name is always exposed by Android's NsdManager, unlike TXT on some
  # devices, so the one-time code rides there too.
  avahi-publish -s "Ryoku Box $CODE" _ryokubox._tcp 8875 "ip=$IP" "pair=$CODE" &
  CHILD=$!

  sleep 110
  cleanup
done
