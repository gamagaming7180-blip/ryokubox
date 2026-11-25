#!/usr/bin/env bash
# Publish the agent over mDNS, including its current IPv4 address in a TXT
# record. Android resolves mDNS to IPv6 on some networks, so the phone reads
# the IPv4 address from TXT instead of guessing.
set -euo pipefail

IP="$(ip -4 -br addr show 2>/dev/null | awk '$3 !~ /^127\./ {sub(/\/.*/,"",$3); print $3; exit}')"
[ -n "$IP" ] || IP="127.0.0.1"

exec avahi-publish -s "Ryoku Box" _ryokubox._tcp 8875 "ip=$IP"
