# Proposal

## Why

Hugo quiere un espacio que siempre esté monitoreando **todas** sus máquinas, no una sola: una app de Teco siempre encendida en la Mac, en la barra de menú y sin ícono en el Dock, que muestre el tablero de toda la red. Además, esa app debe configurarse sola a partir de Tailscale: instalar Teco en una máquina nueva debe bastar para que aparezca en el tablero, sin editar `teco.toml` en ningún lado.

## What Changes

- El vigilante descubre los nodos de la tailnet consultando `tailscale status`, solo cuando alguien pide `/api/devices` y con una caché de 30 s. La vista de Dispositivos los muestra sin configuración. Se filtran teléfonos y tablets, y un nodo sin `tag:teco` solo aparece si alguna vez respondió.
- Nueva app **Teco Monitor** (`monitor/`), un proceso aparte del vigilante:
  - un tecolote en la barra de menú (macOS) o en la bandeja (Windows) que cambia de color con el peor estado de la red;
  - una ventana propia con el tablero de Dispositivos, que libera el motor web al cerrarse;
  - un vigilante de Tailscale: si la app de Tailscale se cerró en macOS, la vuelve a abrir en segundo plano.
- `Teco.app` para macOS, con el vigilante y el monitor dentro, `LSUIElement` (sin Dock) e ícono propio. La CI la publica en el Release `teco-latest`.
- `install.sh` instala `Teco.app` en Aplicaciones y registra dos LaunchAgents: el vigilante, siempre corriendo, y el monitor, que se abre al iniciar sesión y no revive si se cierra con "Salir".
- En macOS las notificaciones salen a nombre de "Teco" cuando `Teco.app` está instalada.

## Capabilities

### New Capabilities
- `fleet-discovery`: descubrimiento automático de los nodos de la tailnet para la vista de Dispositivos.
- `menubar-monitor`: app de barra de menú con el estado de toda la red y la ventana del tablero.

## Impact

- **Vigilante:** suma `serde_json` y el módulo `tailnet.rs`. Binario de ~1.0 MB; con el panel cerrado el costo no cambia.
- **Monitor:** es un binario aparte (`tao`, `wry`, `tray-icon`, `ureq`). Usa unos 30–60 MB solo con la ventana abierta y no afecta al presupuesto del vigilante.
- **CI:** compila el monitor en macOS y Windows, y arma `Teco-macos-arm64.zip`.
- **Pruebas:** la app nativa no se puede probar en la CI ni en Linux. La primera prueba real es en la Mac de Hugo.
