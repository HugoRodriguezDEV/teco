# Spec Delta

## Purpose
Que la vista de Dispositivos muestre sola todas las máquinas de la tailnet que tienen Teco, sin configuración.

## ADDED Requirements

### Requirement: Descubrimiento por Tailscale
Con `discover_tailnet = true` (por defecto), `/api/devices` SHALL incluir, además de los `[[device]]` de `teco.toml`, los nodos de la tailnet que reporta `tailscale status`, con su nombre, su origen HTTPS y si llevan `tag:teco` y están en línea. Los teléfonos y tablets MUST NOT incluirse.

#### Scenario: Máquina nueva
- **WHEN** se instala Teco en una máquina nueva de la tailnet con `tag:teco`
- **THEN** aparece en la vista de Dispositivos de las demás sin editar ningún `teco.toml`

### Requirement: Costo solo con el panel abierto
La consulta a Tailscale SHALL hacerse solo al atender `/api/devices`, y el resultado SHALL reutilizarse durante 30 s.

#### Scenario: Varias peticiones seguidas
- **WHEN** llegan dos peticiones a `/api/devices` con menos de 30 s de diferencia
- **THEN** Tailscale se consulta una sola vez

### Requirement: Nodos sin Teco
Un nodo descubierto sin `tag:teco` SHALL mostrarse solo si alguna vez respondió a `/api/summary`. Un nodo con `tag:teco` que no responde SHALL mostrarse como "Sin conexión".

#### Scenario: Una laptop sin Teco en la tailnet
- **WHEN** la tailnet incluye una máquina sin Teco y sin `tag:teco`
- **THEN** no aparece en la vista de Dispositivos
