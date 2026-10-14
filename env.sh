# Lokale Umgebung für dieses Projekt: Node 24 und npm-Cache nur in diesem Ordner.
# Benutzung: source env.sh
PROJ="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$PROJ/.node/bin:$PROJ/node_modules/.bin:$PATH"
export npm_config_cache="$PROJ/.npm-cache"
# Electron- und electron-builder-Downloads ebenfalls im Projektordner halten statt in ~/.cache.
export ELECTRON_CACHE="$PROJ/.cache/electron"
export electron_config_cache="$PROJ/.cache/electron"
export ELECTRON_BUILDER_CACHE="$PROJ/.cache/electron-builder"
# Android: SDK, Einstellungen und Gradle-Cache ebenfalls im Projektordner statt in ~/.android und ~/.gradle.
export ANDROID_HOME="$PROJ/.android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_USER_HOME="$PROJ/.cache/android"
export GRADLE_USER_HOME="$PROJ/.cache/gradle"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"
