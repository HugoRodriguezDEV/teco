#!/usr/bin/env bash
# Instalador de nodo para macOS (Teco por defecto; reutilizable por Zotz).
# Idempotente: correrlo varias veces deja siempre el mismo estado.
#
#   ./deploy/install.sh                      # Teco con valores por defecto
#   APP=zotz BIN=./zotz SERVE_HTTPS_PORT=8443 SERVE_TARGET=http://127.0.0.1:8788 \
#     LABEL=dev.hugorodriguez.zotz ./deploy/install.sh
#
# Sin clonar nada:
#   curl -fsSL https://raw.githubusercontent.com/HugoRodriguezDEV/teco/HEAD/deploy/install.sh | bash
#
# Si la máquina no está en Tailscale, se abre el inicio de sesión para que entres con TU
# cuenta. Instalación desatendida en una red propia (opcional, nunca va en el repo):
#   ~/.config/zotz-nodes/tailscale.secret   secreto de un OAuth client de Tailscale
#   ~/.config/zotz-nodes/profile.env        TAGS=tag:teco,...  HOSTNAME=...  ALLOW_KILL=true
# Con ambos, la máquina entra sola a esa tailnet, etiquetada y sin abrir sesión.
set -euo pipefail

APP="${APP:-teco}"
LABEL="${LABEL:-dev.hugorodriguez.$APP}"
BIN="${BIN:-./target/release/$APP}"
ARGS="${ARGS:-run}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/Library/Application Support/$APP/bin}"
TAGS="${TAGS:-}"
TS_HOSTNAME="${TS_HOSTNAME:-}"
SERVE_HTTPS_PORT="${SERVE_HTTPS_PORT:-443}"
SERVE_TARGET="${SERVE_TARGET:-http://127.0.0.1:7778}"
SECRET_FILE="${SECRET_FILE:-$HOME/.config/zotz-nodes/tailscale.secret}"
PROFILE_FILE="${PROFILE_FILE:-$HOME/.config/zotz-nodes/profile.env}"
RELEASE_ZIP="${RELEASE_ZIP:-https://github.com/HugoRodriguezDEV/teco/releases/download/teco-latest/Teco-macos-arm64.zip}"
# Teco.app (vigilante + monitor de barra de menú). /Applications si se puede escribir.
APPS_DIR="${APPS_DIR:-$([[ -w /Applications ]] && echo /Applications || echo "$HOME/Applications")}"
# "true"/"false" fija allow_kill_from_panel en teco.toml; vacío no lo toca.
ALLOW_KILL="${ALLOW_KILL:-}"
# Nodos extra ("Nombre=https://..."); los de la tailnet se descubren solos.
DEVICES="${DEVICES:-}"
TS_APP="${TS_APP:-/Applications/Tailscale.app}"

say()  { printf '\033[1m🦉 %s\033[0m\n' "$*"; }
skip() { printf '   ✓ %s\n' "$*"; }
die()  { printf '❌ %s\n' "$*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "Este instalador es para macOS. En Windows usa deploy/install.ps1."

# Perfil local de la red propia: solo datos (CLAVE=valor), nunca se ejecuta.
if [[ -f "$PROFILE_FILE" ]]; then
  while IFS='=' read -r k v; do
    v="$(printf '%s' "$v" | tr -d '\r' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    case "$k" in
      TAGS) [[ -n "$TAGS" ]] || TAGS="$v" ;;
      HOSTNAME) [[ -n "$TS_HOSTNAME" ]] || TS_HOSTNAME="$v" ;;
      ALLOW_KILL) [[ -n "$ALLOW_KILL" ]] || ALLOW_KILL="$v" ;;
    esac
  done < "$PROFILE_FILE"
fi

# ---------------------------------------------------------------- Tailscale
say "Tailscale"
find_ts() {
  if command -v tailscale >/dev/null 2>&1; then command -v tailscale
  elif [[ -x "$TS_APP/Contents/MacOS/Tailscale" ]]; then echo "$TS_APP/Contents/MacOS/Tailscale"
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
if [[ "$st" != "Running" && -d "$TS_APP" ]]; then
  # La app de Tailscale cerrada: se abre en segundo plano (sin ventana) y se espera.
  if ! pgrep -xq Tailscale; then
    open -gj -a "$TS_APP"
    skip "Tailscale estaba cerrado: abierto en segundo plano"
  fi
  for _ in $(seq 1 30); do
    st="$(state || true)"
    [[ "$st" == "Running" || "$st" == "NeedsLogin" || "$st" == "Stopped" ]] && break
    sleep 1
  done
  if [[ "$st" == "Stopped" ]]; then
    # Desconectado a mano: se reconecta con la misma cuenta, sin cambiar nada más.
    "$TS" up >/dev/null 2>&1 || true
    for _ in $(seq 1 15); do st="$(state || true)"; [[ "$st" == "Running" ]] && break; sleep 1; done
    [[ "$st" == "Running" ]] && skip "Tailscale reconectado"
  fi
fi
host_args=()
[[ -n "$TS_HOSTNAME" ]] && host_args=(--hostname="$TS_HOSTNAME")
if [[ "$st" == "Running" ]]; then
  # Ya hay sesión (de usuario o etiquetada): no se toca.
  skip "ya está en la tailnet; no se cambia la sesión"
else
  secret="${TS_OAUTH_SECRET:-}"
  if [[ -z "$secret" && -f "$SECRET_FILE" ]]; then secret="$(tr -d '[:space:]' < "$SECRET_FILE")"; fi
  if [[ -z "$secret" && "${ASK_SECRET:-}" == 1 && -r /dev/tty ]]; then
    printf '   Pega el secreto de Tailscale (tskey-client-...). No se mostrará: '
    read -rs secret < /dev/tty; echo
    secret="$(printf '%s' "$secret" | tr -d '[:space:]')"
    if [[ -n "$secret" ]]; then
      mkdir -p "$(dirname "$SECRET_FILE")"
      (umask 077; printf '%s\n' "$secret" > "$SECRET_FILE")
      skip "secreto guardado en $SECRET_FILE (no se vuelve a pedir)"
    fi
  fi
  if [[ -n "$secret" ]]; then
    # Red propia: unión desatendida y etiquetada.
    [[ -n "$TAGS" ]] || die "Hay secreto de Tailscale pero faltan las etiquetas: ponlas en $PROFILE_FILE (TAGS=tag:teco) o con TAGS=..."
    if [[ "$secret" == tskey-client-* ]]; then secret="${secret}?ephemeral=false&preauthorized=true"; fi
    "$TS" up --auth-key="$secret" --advertise-tags="$TAGS" ${host_args[@]+"${host_args[@]}"}
    unset secret
    skip "unida a tu tailnet con $TAGS (desatendida)"
  else
    # Cualquier otra persona: inicia sesión con SU cuenta de Tailscale.
    printf '   Inicia sesión en Tailscale con tu cuenta: abre el enlace que aparece abajo (o la ventana de Tailscale).\n'
    [[ -d "$TS_APP" ]] && open -a "$TS_APP" || true
    "$TS" up ${host_args[@]+"${host_args[@]}"} || die "No se completó el inicio de sesión en Tailscale. Vuelve a correr el instalador."
    skip "conectada a tu cuenta de Tailscale"
  fi
fi

# Tailscale solo en la barra de menú, sin ícono en el Dock (ajuste propio de la app de Tailscale).
if [[ -d "$TS_APP" ]]; then
  ts_id="$(defaults read "$TS_APP/Contents/Info.plist" CFBundleIdentifier 2>/dev/null || true)"
  if [[ "$ts_id" == io.tailscale.ipn.* ]]; then
    if [[ "$(defaults read "$ts_id" HideDockIcon 2>/dev/null || true)" == 1 ]]; then
      skip "Tailscale ya oculto del Dock"
    else
      defaults write "$ts_id" HideDockIcon -bool true
      skip "Tailscale oculto del Dock (aplica al reabrir Tailscale)"
    fi
  fi
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
changed=0
monitor_changed=0
BUNDLE=""
if [[ "$APP" == "teco" && ! -x "$BIN" ]]; then
  # Teco.app del Release: vigilante + monitor de barra de menú, verificado con SHA-256.
  TMP_DIR="$(mktemp -d)"
  curl -fsSL "$RELEASE_ZIP" -o "$TMP_DIR/teco.zip" || die "No pude descargar $RELEASE_ZIP."
  want="$(curl -fsSL "$RELEASE_ZIP.sha256" | tr -d '[:space:]')" || die "No pude descargar la huella SHA-256."
  got="$(shasum -a 256 < "$TMP_DIR/teco.zip" | cut -d' ' -f1)"
  [[ "$got" == "$want" ]] || { rm -rf "$TMP_DIR"; die "La huella SHA-256 no coincide; no se instala."; }
  ditto -x -k "$TMP_DIR/teco.zip" "$TMP_DIR/x"
  skip "descargado del Release y verificado (SHA-256)"
  mkdir -p "$APPS_DIR"
  BUNDLE="$APPS_DIR/Teco.app"
  NEW="$TMP_DIR/x/Teco.app"
  same() { [[ -f "$2" ]] && [[ "$(shasum -a 256 < "$1")" == "$(shasum -a 256 < "$2")" ]]; }
  same "$NEW/Contents/MacOS/teco" "$BUNDLE/Contents/MacOS/teco" || changed=1
  same "$NEW/Contents/MacOS/teco-monitor" "$BUNDLE/Contents/MacOS/teco-monitor" || monitor_changed=1
  if [[ $changed == 1 || $monitor_changed == 1 ]]; then
    rm -rf "$BUNDLE.new" && ditto "$NEW" "$BUNDLE.new"
    rm -rf "$BUNDLE" && mv "$BUNDLE.new" "$BUNDLE"
    xattr -dr com.apple.quarantine "$BUNDLE" 2>/dev/null || true
    skip "instalado en $BUNDLE"
  else
    skip "sin cambios ($BUNDLE)"
  fi
  rm -rf "$TMP_DIR"
  DEST="$BUNDLE/Contents/MacOS/teco"
else
  # Binario propio (build local o BIN=...): solo el vigilante, sin monitor.
  [[ -x "$BIN" ]] || die "No encuentro el binario $BIN (pásalo con BIN=...)."
  mkdir -p "$INSTALL_DIR"
  DEST="$INSTALL_DIR/$APP"
  if [[ -f "$DEST" ]] && [[ "$(shasum -a 256 < "$BIN")" == "$(shasum -a 256 < "$DEST")" ]]; then
    skip "sin cambios ($DEST)"
  else
    cp "$BIN" "$DEST.new" && chmod 755 "$DEST.new" && mv -f "$DEST.new" "$DEST"
    xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true
    changed=1
    skip "instalado en $DEST"
  fi
fi

# ---------------------------------------------------------------- configuración

if [[ "$APP" == "teco" ]]; then
  say "Configuración"
  # `teco check` crea teco.toml con los valores por defecto solo si no existe.
  "$DEST" check >/dev/null
  CFG="$("$DEST" config-path)"
  cp "$CFG" "$CFG.bak"
  case "$ALLOW_KILL" in
    true|false)
      if grep -qE '^[[:space:]]*allow_kill_from_panel[[:space:]]*=' "$CFG"; then
        sed -E "s/^([[:space:]]*allow_kill_from_panel[[:space:]]*=[[:space:]]*)(true|false)/\1$ALLOW_KILL/" "$CFG.bak" > "$CFG"
      else
        printf 'allow_kill_from_panel = %s\n%s' "$ALLOW_KILL" "$(cat "$CFG")" > "$CFG"
      fi ;;
  esac
  self_dns="$("$TS" status --json 2>/dev/null | grep -o '"DNSName": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)\."$/\1/')"
  for d in $DEVICES; do
    name="${d%%=*}"; url="${d#*=}"
    [[ -n "$self_dns" && "$url" == *"$self_dns"* ]] && continue
    grep -qF "$url" "$CFG" && continue
    printf '\n[[device]]\nname = "%s"\nurl = "%s"\n' "$name" "$url" >> "$CFG"
  done
  if cmp -s "$CFG" "$CFG.bak"; then
    skip "ya al día ($CFG)"
  elif "$DEST" check >/dev/null; then
    skip "actualizada ($CFG)"
  else
    mv -f "$CFG.bak" "$CFG"; die "La configuración resultante no es válida; se dejó la anterior."
  fi
  rm -f "$CFG.bak"
fi

# ---------------------------------------------------------------- arranque automático
domain="gui/$(id -u)"
# ensure_agent <label> <changed 0|1> <keepalive-xml> <extra-xml> <programa> [args...]
ensure_agent() {
  local label="$1" ch="$2" keep="$3" extra="$4"; shift 4
  local plist="$HOME/Library/LaunchAgents/$label.plist" args_xml="" a want
  for a in "$@"; do args_xml+="    <string>$a</string>"$'\n'; done
  want="$(cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$label</string>
  <key>ProgramArguments</key>
  <array>
$args_xml  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key>$keep
  <key>ThrottleInterval</key><integer>10</integer>
$extra  <key>StandardErrorPath</key><string>/tmp/$label.err.log</string>
</dict>
</plist>
EOF
)"
  mkdir -p "$HOME/Library/LaunchAgents"
  if [[ -f "$plist" && "$(cat "$plist")" == "$want" ]] && launchctl print "$domain/$label" >/dev/null 2>&1; then
    if [[ "$ch" == 1 ]]; then
      launchctl kickstart -k "$domain/$label" && skip "$label reiniciado con la versión nueva"
    else
      skip "$label ya registrado y corriendo"
    fi
  else
    launchctl bootout "$domain/$label" 2>/dev/null || true
    printf '%s\n' "$want" > "$plist"
    launchctl bootstrap "$domain" "$plist"
    skip "$label registrado"
  fi
}

say "Arranque automático"
# Vigilante: siempre corriendo, en segundo plano.
# shellcheck disable=SC2086  # ARGS se separa a propósito
ensure_agent "$LABEL" "$changed" "<true/>" \
  "  <key>ProcessType</key><string>Background</string>
  <key>LowPriorityIO</key><true/>
  <key>Nice</key><integer>10</integer>
" "$DEST" $ARGS
if [[ -n "$BUNDLE" ]]; then
  # Monitor de barra de menú: se abre al iniciar sesión; si lo cierras con "Salir", no revive.
  ensure_agent "$LABEL.monitor" "$monitor_changed" "<dict><key>SuccessfulExit</key><false/></dict>" \
    "  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>ProcessType</key><string>Interactive</string>
" "$BUNDLE/Contents/MacOS/teco-monitor"
  # Si ya corría, esto solo trae la ventana al frente (una sola instancia).
  ("$BUNDLE/Contents/MacOS/teco-monitor" >/dev/null 2>&1 &)
  skip "Teco abierto en la barra de menú"
  # Instalación vieja fuera de la app: se retira para no tener dos copias.
  rm -f "$HOME/Library/Application Support/teco/bin/teco" 2>/dev/null || true
fi

say "Listo: https://$("$TS" status --json 2>/dev/null | grep -o '"DNSName": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)\."$/\1/')$( [[ "$SERVE_HTTPS_PORT" == 443 ]] || echo ":$SERVE_HTTPS_PORT")"
