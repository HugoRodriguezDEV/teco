# panel Specification

## Purpose
Responder en 2 segundos a "¿algo anda mal?" con un panel web local que muestra el estado general, las alertas activas, los procesos más pesados y el historial, sin costar nada mientras nadie lo mira.

## Requirements

### Requirement: Servidor del panel
Teco SHALL servir el panel por HTTP en la dirección `bind` de la configuración (por defecto `127.0.0.1:7777`). Si la dirección está ocupada, SHALL terminar con un error que lo explique.

#### Scenario: Puerto ocupado
- **WHEN** otro programa ya escucha en `127.0.0.1:7777`
- **THEN** `teco run` termina con un mensaje que sugiere que otro teco ya corre

### Requirement: Costo cero sin visitantes
El snapshot del panel SHALL construirse solo al recibir una petición. Teco MUST NOT mantener conexiones permanentes, y el navegador SHALL pedir datos solo mientras la pestaña está visible.

#### Scenario: Pestaña oculta
- **WHEN** la pestaña del panel queda en segundo plano
- **THEN** el navegador deja de pedir `/api/state` hasta que vuelve a estar visible

### Requirement: Recursos del panel
`GET /` SHALL devolver la página del panel (un solo archivo HTML con CSS y JS embebidos, sin recursos externos), y `GET /logo.svg`, `/favicon.svg` y `/favicon.ico` SHALL devolver el logo. `HEAD` SHALL responder igual que `GET`, sin cuerpo.

#### Scenario: Sin internet
- **WHEN** el equipo no tiene conexión a internet
- **THEN** el panel carga completo

### Requirement: Estado en JSON
`GET /api/state` SHALL devolver JSON con: versión, host, estado general, métricas del sistema y sus límites, alertas activas y cercanas, los procesos más pesados, los últimos 100 eventos y los datos del propio teco. Acepta `sort=mem|cpu`, `q` (filtro) y `n` (1 a 200, por defecto 25).

#### Scenario: Top por CPU filtrado
- **WHEN** se pide `/api/state?sort=cpu&q=node&n=10`
- **THEN** la respuesta trae hasta 10 procesos cuyo nombre, comando o PID coinciden con "node", ordenados por CPU

### Requirement: Contenido visual
El panel SHALL mostrar arriba el estado como "Tranquilo" (verde), "Atento" (amarillo) o "Alerta" (rojo) con barras de CPU, RAM y swap; después las alertas activas; luego la tabla de procesos con orden por RAM o CPU y filtro de texto; y al final el historial.

#### Scenario: Fila cerca del límite
- **WHEN** un proceso está al 85 % del límite de una regla que le aplica
- **THEN** su fila aparece resaltada en amarillo, y en rojo si pasa el límite

### Requirement: Protección contra DNS rebinding
Teco SHALL rechazar con 403 las peticiones cuyo `Host` no sea loopback (`localhost`, `127.0.0.1`, `::1`), una IP literal o un nombre que termine en `.ts.net`.

#### Scenario: Dominio ajeno
- **WHEN** llega una petición con `Host: evil.example.com:7777`
- **THEN** Teco responde 403

### Requirement: Terminar procesos desde el panel
`POST /api/kill` con `pid` y `start` SHALL terminar el proceso solo si `allow_kill_from_panel = true`, aplicando las mismas protecciones que la acción terminar de las reglas, y SHALL registrar el resultado como evento.

#### Scenario: Función desactivada
- **WHEN** `allow_kill_from_panel = false` y llega una petición válida a `/api/kill`
- **THEN** Teco responde 403 y no termina nada

#### Scenario: PID reciclado
- **WHEN** el `start` enviado no coincide con la hora de inicio actual de ese PID
- **THEN** Teco responde 409 y no termina el proceso

### Requirement: Protección contra CSRF
`POST /api/kill` SHALL exigir la cabecera `X-Teco` y, si la petición trae `Origin`, que su host coincida con `Host`. En otro caso SHALL responder 403.

#### Scenario: Formulario de otro sitio
- **WHEN** una página de otro dominio envía un formulario a `/api/kill`
- **THEN** Teco responde 403

### Requirement: Botón Terminar
El panel SHALL mostrar el botón "Terminar" solo si `allow_kill_from_panel = true`, deshabilitado y con el motivo en los procesos protegidos, y SHALL pedir confirmación antes de enviar la petición.

#### Scenario: Proceso protegido en la tabla
- **WHEN** la tabla muestra el propio proceso de teco
- **THEN** su botón aparece deshabilitado con el motivo "es el propio teco"
