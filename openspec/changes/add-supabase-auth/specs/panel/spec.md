# Spec Delta

## MODIFIED Requirements

### Requirement: Estado en JSON
`GET /api/state` SHALL devolver JSON con: versión, host, estado general, métricas del sistema y sus límites, alertas activas y cercanas, los procesos más pesados, los últimos 100 eventos y los datos del propio teco. Acepta `sort=mem|cpu`, `q` (filtro) y `n` (1 a 200, por defecto 25). Con autenticación activa exige rol `viewer` o superior, salvo desde localhost.

#### Scenario: Top por CPU filtrado
- **WHEN** se pide `/api/state?sort=cpu&q=node&n=10`
- **THEN** la respuesta trae hasta 10 procesos cuyo nombre, comando o PID coinciden con "node", ordenados por CPU

#### Scenario: Sin sesión desde la tailnet
- **WHEN** la autenticación está activa y se pide `/api/state` por el nombre `.ts.net` sin token
- **THEN** Teco responde 401

### Requirement: Terminar procesos desde el panel
`POST /api/kill` con `pid` y `start` SHALL terminar el proceso solo si `allow_kill_from_panel = true`, aplicando las mismas protecciones que la acción terminar de las reglas, y SHALL registrar el resultado como evento. Con autenticación activa exige rol `admin`, salvo desde localhost, y el evento registra el correo de quien lo pidió.

#### Scenario: Función desactivada
- **WHEN** `allow_kill_from_panel = false` y llega una petición válida a `/api/kill`
- **THEN** Teco responde 403 y no termina nada

#### Scenario: PID reciclado
- **WHEN** el `start` enviado no coincide con la hora de inicio actual de ese PID
- **THEN** Teco responde 409 y no termina el proceso

#### Scenario: Viewer intenta terminar
- **WHEN** un usuario con rol `viewer` envía `/api/kill`
- **THEN** Teco responde 403 y no termina nada

### Requirement: Botón Terminar
El panel SHALL mostrar el botón "Terminar" solo si `allow_kill_from_panel = true` y el usuario tiene rol `admin` (o accede desde localhost), deshabilitado y con el motivo en los procesos protegidos, y SHALL pedir confirmación antes de enviar la petición.

#### Scenario: Proceso protegido en la tabla
- **WHEN** la tabla muestra el propio proceso de teco
- **THEN** su botón aparece deshabilitado con el motivo "es el propio teco"

#### Scenario: Usuario viewer
- **WHEN** un usuario con rol `viewer` abre el panel
- **THEN** no ve botones "Terminar"
