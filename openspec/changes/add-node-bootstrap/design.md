# Design

## Context

Teco ya escucha por defecto en `127.0.0.1:7777` y su protección de `Host` ya acepta nombres `*.ts.net` (ver specs `panel`). Hoy no existe la tailnet; la MSI será el primer nodo etiquetado. El repositorio es público, así que ningún secreto puede vivir en él (ver proposal.md).

## Goals / Non-Goals

**Goals:**
- Un comando por máquina, repetible, con resultado idéntico en cada corrida.
- Los mismos scripts sirven para Teco y para Zotz cambiando solo parámetros.
- La política de acceso queda versionada y revisable.

**Non-Goals:**
- Login de usuarios en el panel (eso es `add-supabase-auth`).
- Aplicar la política automáticamente vía API o GitOps: se pega a mano en la consola por ahora.
- Firmar o notarizar binarios.
- Soporte de Linux como nodo.

## Decisions

**OAuth client de Tailscale en vez de auth key.** Las auth keys caducan (máximo 90 días); un OAuth client con permiso de escritura de dispositivos no caduca y su secreto se puede pasar directamente a `tailscale up --auth-key` junto con `--advertise-tags`, con parámetros `?ephemeral=false&preauthorized=true`. Alternativa descartada: auth key reusable, porque obligaría a renovarla cada trimestre en cada nodo.

**Secreto fuera del repo.** Orden de lectura: variable de entorno `TS_OAUTH_SECRET`; si no existe, un archivo local (`~/.config/zotz-nodes/tailscale.secret` en macOS, `%LOCALAPPDATA%\zotz-nodes\tailscale.secret` en Windows) con permisos solo del usuario. El secreto nunca se imprime ni se escribe en logs. Si no hay secreto y el nodo no está en la tailnet, el instalador falla con instrucciones.

**Nodos etiquetados vs. Mac de trabajo.** Un dispositivo etiquetado deja de pertenecer a un usuario. La Mac se usa también para navegar y debe seguir siendo de Hugo, así que el instalador detecta con `tailscale status --json` si ya hay sesión con usuario y, en ese caso, no ejecuta `tailscale up`. La MSI entra etiquetada con `tag:teco,tag:zotz` porque corre ambas apps.

**Publicación con `tailscale serve`.** `tailscale serve --bg --https=<puerto> http://127.0.0.1:<puerto-tailnet-app>` (para Teco, `127.0.0.1:7778`, el listener que siempre exige login; nunca el local `7777`) da HTTPS con un nombre fijo `<host>.<tailnet>.ts.net` sin abrir puertos ni exponer Teco fuera de loopback. Puertos HTTPS: Teco 443, Zotz 8443 (cada app su puerto en el mismo nodo).

**Idempotencia.** Cada paso verifica antes de actuar: Tailscale instalado, nodo unido con las etiquetas esperadas, serve configurado, binario con el mismo hash, config existente (no se sobrescribe), servicio registrado. Repetir el instalador no cambia nada si el estado ya es el deseado.

**Arranque automático.** macOS: LaunchAgent `dev.hugorodriguez.teco` con `KeepAlive`. Windows: tarea programada "Teco" al iniciar sesión, con reinicio cada minuto si falla (como `packaging/windows/install-task.ps1`, que se integra en el instalador).

**Política versionada.** `deploy/tailscale/policy.hujson` define `tagOwners` (Hugo es dueño de `tag:teco` y `tag:zotz`) y `grants`: Hugo llega a todos los puertos de ambas etiquetas; `tag:zotz` puede llegar a `tag:teco` en 443 para leer `/api/state`; nadie más. Un grupo `group:equipo` queda definido vacío para crecer después.

## Risks / Trade-offs

- [Secreto del OAuth client filtrado permite meter nodos a la tailnet] → permisos mínimos (solo escritura de dispositivos con esas etiquetas), archivo con permisos de usuario, rotación documentada.
- [Nodo etiquetado no tiene identidad de usuario en `tailscale serve`] → aceptado; la identidad de usuario la dará Supabase en `add-supabase-auth`.
- [`winget` no disponible en algunas instalaciones de Windows] → el instalador detecta su ausencia y muestra el enlace de descarga del MSI oficial.
- [Política pegada a mano puede divergir del archivo del repo] → la documentación indica que el archivo es la fuente de verdad; automatizarlo queda para después.

## Migration Plan

1. Crear la tailnet, las etiquetas, pegar la política y crear el OAuth client (manual, una vez).
2. MSI: guardar el secreto localmente y correr `install.ps1`.
3. Mac: iniciar sesión en Tailscale con la cuenta de Hugo y correr `install.sh` (no la etiqueta).
4. Rollback: `tailscale serve reset`, desregistrar el servicio, `tailscale logout` en nodos etiquetados.

## Open Questions

- Nombre definitivo de la tailnet y hostnames (`msi-zotz`, `mac-hugo` propuestos): no cambian el diseño.
