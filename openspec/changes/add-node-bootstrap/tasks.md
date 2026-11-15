# Tasks

## 1. Política de la tailnet

- [x] 1.1 Crear `deploy/tailscale/policy.hujson` con `tagOwners` (Hugo dueño de `tag:teco` y `tag:zotz`), `group:equipo` vacío y `grants` (Hugo → todo en ambas etiquetas; `tag:zotz` → `tag:teco` en 443); verificar que es HUJSON válido y que no contiene secretos con `grep -i tskey` sin resultados
- [ ] 1.2 Documentar en `README.md` cómo pegar la política, crear las etiquetas y crear el OAuth client con permiso de escritura de dispositivos limitado a esas etiquetas; verificar siguiendo los pasos en la consola de Tailscale

## 2. Instalador de macOS

- [ ] 2.1 Crear `deploy/install.sh` parametrizado (app, binario, puerto local, puerto HTTPS, etiquetas, hostname) con `set -euo pipefail`; verificar con `shellcheck deploy/install.sh` sin errores
- [ ] 2.2 Instalar Tailscale con `brew` si falta y fallar con instrucciones si no hay `brew`; verificar corriendo dos veces sin reinstalar
- [ ] 2.3 Unir el nodo con `TS_OAUTH_SECRET` o el archivo local solo si no hay sesión, y no tocar una sesión de usuario existente; verificar en la Mac de Hugo que sigue a su nombre tras correr el script
- [ ] 2.4 Configurar `tailscale serve` HTTPS hacia `127.0.0.1:<puerto-tailnet>` (Teco: 7778, nunca 7777) de forma idempotente; verificar con `tailscale serve status` y abriendo el nombre `.ts.net` desde otro nodo
- [ ] 2.5 Instalar el binario, crear `teco.toml` solo si no existe y registrar el LaunchAgent; verificar con `launchctl list | grep dev.hugorodriguez.teco` y matando el proceso para ver que reinicia

## 3. Instalador de Windows

- [ ] 3.1 Crear `deploy/install.ps1` con los mismos parámetros; verificar con `Invoke-ScriptAnalyzer` sin errores
- [ ] 3.2 Instalar Tailscale con `winget` si falta y fallar con instrucciones si no hay `winget`; verificar corriendo dos veces sin reinstalar
- [ ] 3.3 Unir el nodo etiquetado con `--unattended` y nombre fijo usando el secreto local; verificar en la consola de Tailscale que la MSI aparece con `tag:teco` y `tag:zotz`
- [ ] 3.4 Configurar `tailscale serve`, instalar el binario en `%LOCALAPPDATA%\teco`, crear `teco.toml` solo si no existe y registrar la tarea programada con reinicio; verificar abriendo el panel desde la Mac y matando `teco.exe` para ver que reinicia
- [ ] 3.5 Reemplazar `packaging/windows/install-task.ps1` por el nuevo instalador y actualizar `README.md`; verificar que los pasos del README funcionan en la MSI

## 4. Integración

- [ ] 4.1 Probar de punta a punta: MSI instalada por script, Mac con sesión de usuario, panel accesible desde la Mac por HTTPS e inaccesible desde fuera de la tailnet; verificar también que una segunda corrida en ambos nodos no cambia nada
- [ ] 4.2 Preparar la copia de los instaladores para el repositorio de Zotz (parámetros de Zotz documentados); verificar que corre en la MSI junto a Teco sin conflicto de puertos
