# Tasks

## 1. Descubrimiento

- [x] 1.1 `src/tailnet.rs`: leer `tailscale status --json`, filtrar teléfonos y tablets, marcar `tag:teco`, caché de 30 s; verificar con prueba unitaria sobre un JSON real
- [x] 1.2 `/api/devices` combina `[[device]]` manuales con los descubiertos (`auto`, `teco`, `online`) y CSP `https://*.ts.net`; verificar con un `tailscale` simulado (dos peticiones = una consulta)
- [x] 1.3 `devices.html` oculta los nodos descubiertos sin `tag:teco` que nunca respondieron y no intenta los que Tailscale reporta apagados
- [ ] 1.4 Verificar en la tailnet real que la MSI y la Mac aparecen sin `[[device]]`

## 2. Teco Monitor

- [x] 2.1 Crate `monitor/`: ícono en la barra de menú por estado, menú, ventana con el tablero, una sola instancia, vigilante de Tailscale
- [x] 2.2 `Teco.app` (`monitor/macos/`): Info.plist con `LSUIElement`, ícono `.icns`, firma ad hoc y zip; CI y Release
- [x] 2.3 `install.sh` instala `Teco.app` y registra los LaunchAgents del vigilante y del monitor; verificar con simulación (idempotente)
- [ ] 2.4 Probar en la Mac: el ícono cambia de color, la ventana abre y cierra, no aparece en el Dock, cerrar Tailscale lo vuelve a abrir, y reiniciar la sesión abre el monitor
