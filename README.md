# Teco 🦉

*De **tecolote**.* Es un vigilante de recursos ultraligero para macOS y Windows. Corre siempre en segundo plano, casi no consume nada y **solo avisa cuando algo se pasa de la raya**: un Vitest colgado, un proceso que se come 4 GB o una CPU pegada al 100 %.

No es un administrador de tareas. Detecta problemas y tiene un panel mínimo para responder en 2 segundos a la pregunta "¿algo anda mal?".

- Paquete de Cargo: `tecolote` · binario: `teco` · identificador: `dev.hugorodriguez.teco`
- Panel: <http://127.0.0.1:7777>

## Presupuesto (medido)

`teco bench` muestrea durante 60 s, con el panel escuchando pero cerrado, y compara contra el presupuesto. Esta es una corrida real en Linux x86_64, con el build release y unos 120 procesos:

| | Medido | Límite |
|---|---|---|
| RAM (RSS) en reposo | **3.5 MB** | < 15 MB |
| CPU promedio | **0.10 %** | < 0.5 % |
| Binario | **0.8 MB** (2.4 MB con `webhook`) | < 6 MB |
| Arranque hasta la primera muestra | **3 ms** | < 200 ms |

```bash
cargo run --release -- bench          # 60 s
```

```bash
cargo run --release -- bench --secs 20
```

Cómo se consigue:

- Un solo `System` de `sysinfo`, creado una vez. Solo se refresca CPU y memoria (`ProcessRefreshKind::nothing().with_cpu().with_memory()`).
- La línea de comando se lee **una sola vez por PID** (`UpdateKind::OnlyIfNotSet`). Nombre y comando se guardan como `Arc<str>`, así que el ciclo caliente no copia strings.
- Qué reglas aplican a cada proceso se calcula una vez por PID con un bitmask `u64`. Solo se recalcula si cambia `teco.toml`.
- Los buffers (`Vec` de filas, mapas PID→índice y agregados por árbol) se reutilizan entre muestras.
- El servidor HTTP es propio: síncrono, un hilo y sin dependencias. Duerme en `accept()`. El JSON se arma **solo cuando llega una petición**. No usa websockets: el panel hace *polling* únicamente mientras la pestaña está visible (`document.visibilityState`).
- El panel es un solo `index.html` con CSS y JS vanilla, embebido con `include_str!`. No usa CDNs.
- No hay runtime async. Probé que un servidor síncrono pequeño sale más ligero que tokio y se queda así.
- La recarga en caliente es un `stat` del archivo por muestra; no hay watcher.
- En Linux, `sysinfo` lista los hilos como procesos. Teco los descarta.

## Uso

```text
teco [run] [--config <ruta>]     vigila en segundo plano y sirve el panel
teco bench [--secs N]            mide RSS/CPU propios contra el presupuesto
teco check [--config <ruta>]     valida la config y muestra las reglas
teco config-path                 imprime dónde está teco.toml
```

La primera vez se crea `teco.toml` con los valores por defecto ([`assets/default.toml`](assets/default.toml)) en:

- macOS: `~/Library/Application Support/teco/teco.toml`
- Windows: `%APPDATA%\teco\teco.toml`

El registro de eventos `teco-events.log` (rotativo, 1 MB × 3) queda en la misma carpeta.

## Reglas

```toml
[[rule]]
name = "Vitest colgado"
match = "vitest"          # "*" = cualquiera; si no, substring sin mayúsculas en nombre o comando
include_children = true   # suma RAM y CPU del árbol (padre + workers)
max_runtime_min = 40      # también: max_mem_gb, max_cpu_percent (se puede combinar)
for_secs = 0              # cuánto debe sostenerse antes de disparar
cooldown_min = 15         # si sigue arriba, vuelve a avisar tras esto (0 = nunca)
action = "alert"          # "alert" | "kill"
```

- **Sostenido**: la regla se dispara solo si la condición dura `for_secs` seguidos. Los picos no cuentan.
- **Una vez** por (regla, proceso). Se vuelve a armar cuando el proceso baja del límite durante `for_secs`, o avisa de nuevo tras `cooldown_min` si sigue arriba. El proceso se identifica por PID + hora de inicio, así que un PID reciclado no se confunde.
- **`include_children`**: cada proceso cuenta dentro de su ancestro coincidente **más alto**. Si un worker también dice "vitest", suma en su raíz y no se cuenta doble.
- **Estado del panel**:
  - **Tranquilo**: nada se acerca a un límite.
  - **Atento**: algo está arriba del 80 % de su límite, o ya lo pasó pero aún no se sostiene `for_secs`.
  - **Alerta**: hay alguna regla disparada.
- **`[system]`**: RAM total (%), swap (GB) y CPU global (%), con su propio `for_secs`.
- **Recarga en caliente**: al guardar `teco.toml`, Teco lo vuelve a leer. Si el archivo tiene un error, se conserva la config anterior y el error aparece en el historial. Cambiar `bind` requiere reiniciar.

### `action = "kill"`

Teco nunca mata a estos procesos:

- a sí mismo,
- PID 0, 1 o 4,
- nada que esté en `protected` (por nombre exacto, sin distinguir mayúsculas).

Antes de matar, **avisa y registra el evento**. En Unix manda `SIGTERM` y, si el proceso sigue vivo a los 5 s, `SIGKILL`. En Windows usa `TerminateProcess`. Con `include_children` se termina todo el árbol, saltando a los protegidos.

## Avisos

- **Notificación nativa** (`notify-rust` en macOS y Windows). Ejemplo: título `🦉 Teco · Vitest colgado`, cuerpo `bun.exe (10918) · 43 min (límite 40 min) · 3.1 GB · 3 procesos`.
- **Historial**: los últimos 200 eventos en memoria, para el panel, más `teco-events.log` en disco.
- **Webhook** (opcional, desactivado por defecto): compila con `cargo build --release --features webhook` y pon `webhook_url = "https://ntfy.sh/tu-tema"` en `teco.toml`. Teco hace un POST con JSON que incluye `source`, `host`, `title`, `body`, `rule`, `pid` y `action`.

## Panel

Ve a `http://127.0.0.1:7777`. Muestra:

- el estado general y las barras de CPU, RAM y swap, con una marca en el límite;
- las alertas activas;
- el top 25 por RAM o por CPU, con filtro;
- el historial de eventos.

Las filas que se acercan a un límite se resaltan en amarillo, y las que lo pasan, en rojo. El panel funciona en modo claro y oscuro, y en el teléfono.

El botón **Terminar** solo aparece con `allow_kill_from_panel = true`. Tiene estas protecciones:

- pide confirmación;
- aplica las mismas protecciones que `kill`;
- rechaza el PID si ya pertenece a otro proceso;
- exige la cabecera `X-Teco` y un `Origin` igual al `Host`, contra CSRF;
- valida `Host` contra DNS rebinding. Acepta loopback, IPs literales y `*.ts.net`, así que funciona por Tailscale si pones `bind = "0.0.0.0:7777"` o la IP de Tailscale.

## Arranque automático

**macOS (launchd):**

```bash
cargo build --release
```

```bash
sudo cp target/release/teco /usr/local/bin/teco
```

```bash
cp packaging/macos/dev.hugorodriguez.teco.plist ~/Library/LaunchAgents/
```

```bash
launchctl load -w ~/Library/LaunchAgents/dev.hugorodriguez.teco.plist
```

**Windows (Programador de tareas, al iniciar sesión, se reinicia si se cae):**

```powershell
cargo build --release
mkdir $env:LOCALAPPDATA\teco -Force; copy target\release\teco.exe $env:LOCALAPPDATA\teco\
.\packaging\windows\install-task.ps1
```

En Windows, `teco.exe` corre sin ventana de consola. Cuando lo lanzas desde una terminal (`teco bench`, `teco check`) se engancha a ella para imprimir.

## Notas por plataforma

- **macOS**: si `teco` corre como binario suelto (fuera de un `.app`), macOS puede atribuir la notificación a otra app o pedir permiso de notificaciones la primera vez.
- **Windows**: las notificaciones salen como *toast* de Windows. Un nodo 24/7 necesita una sesión iniciada para verlas. El panel y el log funcionan siempre.
- **CPU por proceso**: se mide en % de **un** núcleo, así que puede pasar de 100 en procesos multihilo.

## Desarrollo

```bash
cargo test
```

```bash
cargo clippy --release --all-features
```

```bash
cargo run -- --config ./dev.toml
```

La CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) compila, prueba y corre `teco bench` en macOS, Windows y Linux. Es gratis mientras el repo sea público; si lo haces privado, consume minutos de tu cuota (macOS es el más caro).

Para verificar a mano en tu Mac o en la MSI:

```bash
cargo test --release && cargo build --release && cargo run --release -- bench
```
