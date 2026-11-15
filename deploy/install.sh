#!/usr/bin/env bash
# Instalador de nodo para macOS (Teco por defecto; reutilizable por Zotz).
# Idempotente: correrlo varias veces deja siempre el mismo estado.
#
#   ./deploy/install.sh                      # Teco con valores por defecto
#   APP=zotz BIN=./zotz SERVE_HTTPS_PORT=8443 SERVE_TARGET=http://127.0.0.1:8788 \
#     LABEL=dev.hugorodriguez.zotz ./deploy/install.sh
#
# El secreto de Tailscale (solo para máquinas sin sesión) se lee de la variable
# TS_OAUTH_SECRET o de ~/.config/zotz-nodes/tailscale.secret. Nunca se imprime.
set -euo pipefail

APP="${APP:-teco}"
LABEL="${LABEL:-dev.hugorodriguez.$APP}"
BIN="${BIN:-./target/release/$APP}"
ARGS="${ARGS:-run}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/Library/Application Support/$APP/bin}"
TAGS="${TAGS:-tag:$APP}"
TS_HOSTNAME="${TS_HOSTNAME:-mac-hugo}"
SERVE_HTTPS_PORT="${SERVE_HTTPS_PORT:-443}"
SERVE_TARGET="${SERVE_TARGET:-http://127.0.0.1:7778}"
SECRET_FILE="${SECRET_FILE:-$HOME/.config/zotz-nodes/tailscale.secret}"
TAILNET="tail59ed16.ts.net"

say()  { printf '\033[1m🦉 %s\033[0m\n' "$*"; }
skip() { printf '   ✓ %s\n' "$*"; }
die()  { printf '❌ %s\n' "$*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "Este instalador es para macOS. En Windows usa deploy/install.ps1."

# ---------------------------------------------------------------- Tailscale
say "Tailscale"
find_ts() {
  if command -v tailscale >/dev/null 2>&1; then command -v tailscale
  elif [[ -x /Applications/Tailscale.app/Contents/MacOS/Tailscale ]]; then echo /Applications/Tailscale.app/Contents/MacOS/Tailscale
  fi
}
TS="$(find_ts || true)"
if [[ -z "$TS" ]]; then
  command -v brew >/dev/null 2>&1 || die "Falta Tailscale y no hay Homebrew. Instala Tailscale desde la App Store o https://tailscale.com/download/mac y vuelve a correr esto."
  brew install --cask tailscale
  open -a Tailscale || true
  TS="$(find_ts || true)"
  [[ -n "$TS" ]] || die "Tailscale se instaló pero no encuentro su CLI. Abre la app una vez y vuelve a correr esto."
else
  skip "instalado ($TS)"
fi

state() { "$TS" status --json 2>/dev/null | grep -o '"BackendState": *"[A-Za-z]*"' | head -1 | sed 's/.*"\([A-Za-z]*\)"$/\1/'; }
st="$(state || true)"
if [[ "$st" == "Running" ]]; then
  # Ya hay sesión (de usuario o etiquetada): no se toca.
  skip "ya está en la tailnet; no se cambia la sesión"
else
  secret="${TS_OAUTH_SECRET:-}"
  if [[ -z "$secret" && -f "$SECRET_FILE" ]]; then secret="$(tr -d '[:space:]' < "$SECRET_FILE")"; fi
  [[ -n "$secret" ]] || die "La máquina no está en la tailnet y no hay secreto. Define TS_OAUTH_SECRET o guárdalo en $SECRET_FILE (chmod 600)."
  # Un secreto de OAuth client necesita estos parámetros; una auth key normal no.
  if [[ "$secret" == tskey-client-* ]]; then secret="${secret}?ephemeral=false&preauthorized=true"; fi
  "$TS" up --auth-key="$secret" --advertise-tags="$TAGS" --hostname="$TS_HOSTNAME"
  unset secret
  skip "unida como $TS_HOSTNAME.$TAILNET con $TAGS"
fi

# ---------------------------------------------------------------- tailscale serve
say "Publicación en la tailnet (https:$SERVE_HTTPS_PORT → $SERVE_TARGET)"
if "$TS" serve status 2>/dev/null | grep -q "${SERVE_TARGET#http://}"; then
  skip "ya publicado"
else
  "$TS" serve --bg --https="$SERVE_HTTPS_PORT" "$SERVE_TARGET"
fi

# ---------------------------------------------------------------- binario
say "Binario"
if [[ ! -x "$BIN" && "$APP" == "teco" && -f Cargo.toml ]]; then
  command -v cargo >/dev/null 2>&1 || die "No encuentro $BIN ni cargo. Instala Rust (https://rustup.rs) o pasa BIN=/ruta/al/binario."
  cargo build --release
fi
[[ -x "$BIN" ]] || die "No encuentro el binario $BIN (pásalo con BIN=...)."
mkdir -p "$INSTALL_DIR"
DEST="$INSTALL_DIR/$APP"
changed=0
if [[ -f "$DEST" ]] && [[ "$(shasum -a 256 < "$BIN")" == "$(shasum -a 256 < "$DEST")" ]]; then
  skip "sin cambios ($DEST)"
else
  cp "$BIN" "$DEST.new" && chmod 755 "$DEST.new" && mv -f "$DEST.new" "$DEST"
  xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true
  changed=1
  skip "instalado en $DEST"
fi

# ---------------------------------------------------------------- configuración
if [[ "$APP" == "teco" ]]; then
  say "Configuración"
  # `teco check` crea teco.toml con los valores por defecto solo si no existe.
  "$DEST" check >/dev/null && skip "$("$DEST" config-path)"
fi

# ---------------------------------------------------------------- arranque automático
say "Arranque automático ($LABEL)"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
mkdir -p "$HOME/Library/LaunchAgents"
args_xml=""
for a in $ARGS; do args_xml+="    <string>$a</string>"$'\n'; done
want="$(cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$DEST</string>
$args_xml  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>10</integer>
  <key>ProcessType</key><string>Background</string>
  <key>LowPriorityIO</key><true/>
  <key>Nice</key><integer>10</integer>
  <key>StandardErrorPath</key><string>/tmp/$APP.err.log</string>
</dict>
</plist>
EOF
)"
domain="gui/$(id -u)"
if [[ -f "$PLIST" && "$(cat "$PLIST")" == "$want" ]] && launchctl print "$domain/$LABEL" >/dev/null 2>&1; then
  if [[ $changed == 1 ]]; then
    launchctl kickstart -k "$domain/$LABEL" && skip "reiniciado con el binario nuevo"
  else
    skip "ya registrado y corriendo"
  fi
else
  launchctl bootout "$domain/$LABEL" 2>/dev/null || true
  printf '%s\n' "$want" > "$PLIST"
  launchctl bootstrap "$domain" "$PLIST"
  skip "registrado ($PLIST)"
fi

say "Listo: https://$("$TS" status --json 2>/dev/null | grep -o '"DNSName": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)\."$/\1/')$( [[ "$SERVE_HTTPS_PORT" == 443 ]] || echo ":$SERVE_HTTPS_PORT")"
