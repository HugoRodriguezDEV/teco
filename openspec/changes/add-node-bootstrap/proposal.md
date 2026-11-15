# Proposal

## Why

Cada vez que se instala Teco (o Zotz) en una máquina hay que meterla a mano a la red de Tailscale, configurar el panel y dejarlo arrancando solo. Hugo quiere que instalar sea un solo comando y que el resultado sea **siempre exactamente el mismo** en la Mac y en la MSI, sin tocar nada a mano. Es también la base para el login con Supabase (`add-supabase-auth`), que necesita un nombre HTTPS fijo por nodo.

## What Changes

- Nuevos instaladores idempotentes: `install.sh` (macOS) e `install.ps1` (Windows). Están pensados para copiarse tal cual al repositorio de Zotz, cambiando solo el nombre de la app, el puerto y la etiqueta.
- El instalador instala Tailscale si falta, une el nodo a la tailnet con una etiqueta (`tag:teco`, `tag:zotz`) y un nombre fijo usando un secreto de OAuth client de Tailscale, y publica el panel dentro de la tailnet por HTTPS con `tailscale serve`.
- El secreto de Tailscale se lee de una variable de entorno o de un archivo local fuera del repositorio; nunca se versiona.
- Si la máquina ya tiene sesión de Tailscale con una cuenta de usuario (la Mac de trabajo), el instalador no cambia esa sesión ni le pone etiquetas.
- El instalador copia el binario, escribe la configuración predeterminada idéntica en todos los nodos (sin sobrescribir una existente) y registra el arranque automático (launchd en macOS, Programador de tareas en Windows).
- Teco queda escuchando solo en `127.0.0.1`; desde la tailnet se llega por el nombre HTTPS de `tailscale serve`.
- Nuevo archivo versionado de política de la tailnet: `deploy/tailscale/policy.hujson`, con los dueños de las etiquetas y quién puede llegar a qué.

## Capabilities

### New Capabilities
- `node-bootstrap`: instalación desatendida e idempotente de un nodo (Tailscale, unión a la tailnet, `tailscale serve`, binario, configuración y arranque automático) en macOS y Windows.
- `tailnet-access`: la política de acceso de la tailnet como archivo versionado: etiquetas, dueños y qué identidades llegan a qué nodos y puertos.

### Modified Capabilities
<!-- Ninguna: el panel ya escucha en 127.0.0.1 por defecto y ya acepta Host *.ts.net. -->

## Impact

- Archivos nuevos: `deploy/install.sh`, `deploy/install.ps1`, `deploy/tailscale/policy.hujson`, documentación en `README.md`.
- Sin cambios en el binario de Teco ni en su presupuesto.
- Dependencias externas en el nodo: Tailscale (instalado con `brew` o `winget`).
- Configuración manual única en la consola de Tailscale: crear las etiquetas, pegar la política y crear el OAuth client.
- Zotz: recibe una copia de los instaladores; su integración ocurre en su propio repositorio.
- Habilita `add-supabase-auth`, que depende del nombre HTTPS fijo de cada nodo.
