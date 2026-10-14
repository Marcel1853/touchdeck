#!/bin/sh
# Baut die Android-App und legt sie unter downloads/ ab, von wo TouchDeck sie den Handys anbietet.
set -e
DIR="$(cd "$(dirname "$0")/.." && pwd)"
. "$DIR/../env.sh"
cd "$DIR/../android"
./gradlew --no-daemon -q assembleRelease
cp app/build/outputs/apk/release/app-release.apk "$DIR/downloads/TouchDeck.apk"
echo "Fertig: $DIR/downloads/TouchDeck.apk ($(du -h "$DIR/downloads/TouchDeck.apk" | cut -f1))"
