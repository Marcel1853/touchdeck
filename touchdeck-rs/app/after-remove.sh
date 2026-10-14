#!/bin/sh
# TouchDeck: Firewall-Regeln wieder entfernen.
if command -v ufw >/dev/null 2>&1; then
  for NET in 192.168.0.0/16 10.0.0.0/8 172.16.0.0/12; do
    ufw delete allow from "$NET" to any port 8770 proto tcp >/dev/null 2>&1 || true
    ufw delete allow from "$NET" to any port 8769 proto udp >/dev/null 2>&1 || true
  done
fi
exit 0
