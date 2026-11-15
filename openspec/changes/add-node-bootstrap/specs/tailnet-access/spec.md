# Spec Delta

## Purpose
Mantener la política de acceso de la tailnet privada como un archivo versionado en el repositorio, para que quién llega a qué nodo y puerto sea explícito, revisable y siempre igual.

## ADDED Requirements

### Requirement: Política versionada
El repositorio SHALL contener `deploy/tailscale/policy.hujson` como fuente de verdad de la política de la tailnet. El archivo MUST NOT contener secretos.

#### Scenario: Revisar quién tiene acceso
- **WHEN** alguien quiere saber quién puede abrir el panel de Teco
- **THEN** lo encuentra en `deploy/tailscale/policy.hujson` sin entrar a la consola de Tailscale

### Requirement: Dueños de las etiquetas
La política SHALL declarar a Hugo como dueño de `tag:teco` y `tag:zotz`, de modo que solo él (o un OAuth client creado por él) pueda asignarlas.

#### Scenario: Etiqueta no autorizada
- **WHEN** un dispositivo intenta anunciar `tag:teco` sin estar autorizado por su dueño
- **THEN** Tailscale rechaza la etiqueta

### Requirement: Acceso del administrador
La política SHALL permitir a la cuenta de Hugo llegar a todos los puertos de los nodos con `tag:teco` y `tag:zotz`.

#### Scenario: Hugo abre el panel desde la Mac
- **WHEN** Hugo abre `https://msi-zotz.<tailnet>.ts.net` desde su Mac
- **THEN** la conexión llega al panel de Teco en la MSI

### Requirement: Zotz consulta a Teco
La política SHALL permitir que los nodos `tag:zotz` lleguen a los nodos `tag:teco` solo en el puerto HTTPS del panel, para leer el estado de recursos.

#### Scenario: Zotz revisa recursos antes de lanzar una tarea
- **WHEN** un nodo de Zotz pide `/api/state` al panel de Teco por HTTPS
- **THEN** la petición llega; cualquier otro puerto de ese nodo queda bloqueado para Zotz

### Requirement: Denegación por defecto
Cualquier identidad o etiqueta no listada explícitamente en la política MUST NOT llegar a los nodos de Teco ni de Zotz. La política SHALL incluir un grupo `group:equipo` vacío, listo para dar acceso futuro.

#### Scenario: Integrante del equipo sin permiso
- **WHEN** un nuevo integrante entra a la tailnet pero no está en `group:equipo` ni en otra regla
- **THEN** no puede llegar a ningún nodo de Teco o Zotz
