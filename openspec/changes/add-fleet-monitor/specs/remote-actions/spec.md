# Spec Delta

## Purpose
Detener o matar, desde el tablero, los procesos que se pasan de sus límites en cualquier máquina de la tailnet.

## ADDED Requirements

### Requirement: Botonera en el tablero
Cada tarjeta de la vista de Dispositivos SHALL listar los procesos que se pasan, o están cerca de pasarse, de una regla (hasta 8): nombre, PID, regla y valor contra límite. Si `allow_kill_from_panel = true` en esa máquina, cada uno SHALL tener los botones "Detener" y "Matar", que piden confirmación. Si no, SHALL indicar que Terminar está desactivado ahí.

#### Scenario: Vitest colgado en la MSI
- **WHEN** la regla "Vitest colgado" se dispara en la MSI y Hugo hace clic en "Matar" desde el tablero de su Mac
- **THEN** tras confirmar, la MSI termina el proceso y sus hijos, y el tablero muestra el resultado

### Requirement: Detener y matar
"Detener" SHALL enviar SIGTERM y SIGKILL a los 5 s si el proceso sigue vivo; "Matar" SHALL enviar SIGKILL de inmediato. En Windows ambos SHALL usar `TerminateProcess`. Si la alerta agrupa un árbol (`include_children`), la acción SHALL incluir a los descendientes. Las protecciones existentes (el propio teco, PID 0/1/4, `protected`, PID reciclado) MUST aplicarse a cada proceso.

#### Scenario: Proceso protegido
- **WHEN** el proceso que se pasa está en `protected`
- **THEN** la tarjeta lo muestra sin botones, con el motivo

### Requirement: Orígenes permitidos
`POST /api/kill` SHALL aceptar peticiones de otra página solo si traen la cabecera `X-Teco` y su origen es loopback, o es de la misma tailnet que el `Host` por el que entraron (mismo sufijo `*.ts.net`, o IPs de Tailscale). Por loopback solo se aceptan orígenes de loopback. Otros `*.ts.net`, como los sitios públicos de Tailscale Funnel, MUST rechazarse.

#### Scenario: Sitio público de Funnel
- **WHEN** una página de `https://otro.tailxyz.ts.net` intenta `POST /api/kill` contra `msi-zotz.tail59ed16.ts.net`
- **THEN** la petición se rechaza con 403
