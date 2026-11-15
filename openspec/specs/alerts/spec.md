# alerts Specification

## Purpose
Hacer ruido solo cuando hay algo: notificar al usuario cuando una regla se dispara, dejar registro de lo que pasó (en memoria para el panel y en disco) y, opcionalmente, reenviar el aviso a un servicio externo.

## Requirements

### Requirement: Notificación nativa
Al dispararse una regla, Teco SHALL mostrar una notificación nativa del sistema si `notifications = true` (valor por defecto). El título SHALL ser "🦉 Teco · <nombre de la regla>".

#### Scenario: Notificaciones desactivadas
- **WHEN** `notifications = false` y se dispara una regla
- **THEN** no aparece notificación, pero el evento sí se registra

### Requirement: Contenido del aviso
El cuerpo del aviso SHALL incluir el nombre del proceso, su PID, el valor actual contra el límite y, cuando aplique: la RAM si la métrica no es memoria, el número de procesos del grupo, desde hace cuánto está arriba, "sigue así" si es repetición y "terminando…" si la acción es terminar.

#### Scenario: Vitest colgado
- **WHEN** se dispara "Vitest colgado" para `bun.exe` (10918) con 43 min de 40 permitidos y 3.1 GB en 3 procesos
- **THEN** el cuerpo dice "bun.exe (10918) · 43 min (límite 40 min) · 3.1 GB · 3 procesos"

### Requirement: Historial en memoria
Teco SHALL conservar en memoria los últimos 200 eventos (alertas, terminaciones, resoluciones, información y errores), con hora, tipo, título, texto y PID, para mostrarlos en el panel del más reciente al más antiguo.

#### Scenario: Más de 200 eventos
- **WHEN** ocurren 250 eventos
- **THEN** el panel muestra solo los 200 más recientes

### Requirement: Registro en disco rotativo
Teco SHALL escribir cada evento como una línea con hora UTC en `teco-events.log`, en la carpeta de la configuración. Cuando el archivo llega a 1 MB SHALL rotarlo, conservando como máximo 3 archivos.

#### Scenario: Rotación
- **WHEN** `teco-events.log` alcanza 1 MB
- **THEN** pasa a ser `teco-events.log.1`, el `.1` anterior pasa a `.2`, el `.2` anterior se descarta y se empieza un archivo nuevo

### Requirement: Webhook opcional
Solo en builds con la feature `webhook` y con `webhook_url` configurado, Teco SHALL enviar un POST JSON por cada aviso, con los campos `source`, `host`, `title`, `body`, `rule`, `pid` y `action`. Sin la feature, `webhook_url` se ignora.

#### Scenario: Aviso a ntfy
- **WHEN** el build incluye `webhook` y `webhook_url = "https://ntfy.sh/mi-tema"`
- **THEN** cada regla disparada genera un POST JSON a esa URL

### Requirement: Avisos sin bloquear el muestreo
Enviar notificaciones o webhooks MUST NOT retrasar la siguiente muestra: si el sistema de notificaciones o la red tardan, el muestreo continúa a tiempo.

#### Scenario: Webhook lento
- **WHEN** el servidor del webhook tarda 10 s en responder
- **THEN** las muestras siguientes se toman en su intervalo normal
