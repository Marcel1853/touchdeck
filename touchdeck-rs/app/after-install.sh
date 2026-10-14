#!/bin/sh
# TouchDeck: Firewall (ufw) für Handys im Heimnetz öffnen – Bedienfeld (TCP 8770) und
# automatisches Finden (UDP 8769), nur aus privaten Netzen. Wird beim Deinstallieren entfernt.
if command -v ufw >/dev/null 2>&1; then
  for NET in 192.168.0.0/16 10.0.0.0/8 172.16.0.0/12; do
    ufw allow from "$NET" to any port 8770 proto tcp comment 'TouchDeck' >/dev/null 2>&1 || true
    ufw allow from "$NET" to any port 8769 proto udp comment 'TouchDeck' >/dev/null 2>&1 || true
  done
fi
exit 0
