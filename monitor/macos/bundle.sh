#!/usr/bin/env bash
# Arma Teco.app (CI de macOS). Uso: monitor/macos/bundle.sh <dir-salida>
set -euo pipefail
OUT="${1:-dist}"
APP="$OUT/Teco.app"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp monitor/macos/Info.plist "$APP/Contents/Info.plist"
cp target/release/teco-monitor "$APP/Contents/MacOS/teco-monitor"
# El vigilante viaja dentro de la app: un solo paquete que instalar o borrar.
cp target/release/teco "$APP/Contents/MacOS/teco"
ICONSET="$(mktemp -d)/teco.iconset"
mkdir -p "$ICONSET"
I=monitor/icons
cp $I/app-16.png   "$ICONSET/icon_16x16.png"
cp $I/app-32.png   "$ICONSET/icon_16x16@2x.png"
cp $I/app-32.png   "$ICONSET/icon_32x32.png"
cp $I/app-64.png   "$ICONSET/icon_32x32@2x.png"
cp $I/app-128.png  "$ICONSET/icon_128x128.png"
cp $I/app-256.png  "$ICONSET/icon_128x128@2x.png"
cp $I/app-256.png  "$ICONSET/icon_256x256.png"
cp $I/app-512.png  "$ICONSET/icon_256x256@2x.png"
cp $I/app-512.png  "$ICONSET/icon_512x512.png"
cp $I/app-1024.png "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/teco.icns"
# Firma ad hoc (sin certificado de Apple): basta para que arm64 la ejecute.
codesign --force --deep --sign - "$APP"
(cd "$OUT" && ditto -c -k --keepParent Teco.app Teco-macos-arm64.zip)
echo "listo: $OUT/Teco-macos-arm64.zip"
