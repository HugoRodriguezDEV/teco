# Spec Delta

## Purpose
Un espacio siempre encendido en la Mac que muestra el estado de toda la red y abre el tablero con un clic.

## ADDED Requirements

### Requirement: Ícono de estado
El monitor SHALL mostrar un tecolote en la barra de menú cuyo color refleje el peor estado de la red: normal (tranquilo), amarillo (atento), rojo (alerta o un nodo con `tag:teco` sin conexión) y gris con punto rojo (sin Tailscale o sin el vigilante local).

#### Scenario: La MSI se pasa de RAM
- **WHEN** la MSI tiene una regla disparada
- **THEN** el tecolote de la Mac se pone rojo y el menú dice qué máquina está en alerta

### Requirement: Sin Dock
El monitor MUST NOT mostrar ícono en el Dock ni en Cmd+Tab; vive solo en la barra de menú.

#### Scenario: Monitor abierto
- **WHEN** el monitor corre con la ventana cerrada
- **THEN** solo se ve el tecolote en la barra de menú

### Requirement: Ventana del tablero
"Abrir tablero" SHALL mostrar la vista de Dispositivos en una ventana propia. Cerrar la ventana SHALL liberar el motor web sin cerrar el monitor.

#### Scenario: Cerrar la ventana
- **WHEN** se cierra la ventana del tablero
- **THEN** el tecolote sigue en la barra de menú y sigue cambiando de color

### Requirement: Siempre encendido
El instalador SHALL registrar el monitor para abrirse al iniciar sesión. Si el monitor termina por un error, SHALL volver a arrancar; si se cierra con "Salir", MUST NOT volver a arrancar hasta el siguiente inicio de sesión.

#### Scenario: Reinicio de la Mac
- **WHEN** Hugo inicia sesión en la Mac
- **THEN** el monitor se abre solo con el tablero

### Requirement: Tailscale siempre arriba
En macOS, si la app de Tailscale no está corriendo, el monitor SHALL abrirla en segundo plano (como máximo una vez por minuto). El monitor MUST NOT reconectar una sesión que el usuario desconectó a propósito; en ese caso SHALL mostrar el estado "Tailscale está desconectado".

#### Scenario: Se cerró Tailscale
- **WHEN** Hugo cierra la app de Tailscale
- **THEN** en menos de un minuto el monitor la vuelve a abrir y la red se ve otra vez
