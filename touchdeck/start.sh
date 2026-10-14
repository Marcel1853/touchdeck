#!/bin/sh
# Startet TouchDeck. Nutzt das Node aus dem Projektordner, falls vorhanden, sonst das System-Node.
DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -x "$DIR/../.node/bin/node" ]; then
  NODE="$DIR/../.node/bin/node"
else
  NODE="$(command -v node)" || { echo "Node.js (ab Version 22) wird benötigt." >&2; exit 1; }
fi
exec "$NODE" "$DIR/src/main.js" "$@"
