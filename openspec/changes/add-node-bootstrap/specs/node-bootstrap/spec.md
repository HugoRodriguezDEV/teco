# Spec Delta

## Purpose
Instalar un nodo (Teco o Zotz) en macOS o Windows con un solo comando desatendido e idempotente, que lo une a la tailnet privada y lo deja publicado y arrancando solo, siempre con el mismo resultado.

## ADDED Requirements

### Requirement: Instalación con un comando
El proyecto SHALL proveer `install.sh` para macOS e `install.ps1` para Windows que, sin preguntas interactivas, dejan el nodo listo: Tailscale instalado y unido, panel publicado en la tailnet, binario instalado, configuración presente y arranque automático registrado.

#### Scenario: MSI nueva
- **WHEN** se ejecuta `install.ps1` en una MSI sin Tailscale ni Teco y con el secreto de Tailscale disponible localmente
- **THEN** al terminar Teco corre, arranca solo al iniciar sesión y su panel responde en `https://<host>.<tailnet>.ts.net`

### Requirement: Idempotencia
Ejecutar el instalador varias veces SHALL producir el mismo estado final. Un paso cuyo resultado ya existe MUST NOT repetirse ni modificar lo existente.

#### Scenario: Segunda ejecución
- **WHEN** se ejecuta el instalador en un nodo ya instalado
- **THEN** no se reinstala Tailscale, no se vuelve a unir el nodo y no se sobrescribe `teco.toml`

### Requirement: Instalación de Tailscale
Si Tailscale no está instalado, el instalador SHALL instalarlo con `brew` en macOS o `winget` en Windows. Si el gestor de paquetes no existe, SHALL fallar con instrucciones para instalarlo a mano.

#### Scenario: Windows sin winget
- **WHEN** se ejecuta `install.ps1` en un Windows sin `winget` ni Tailscale
- **THEN** el instalador se detiene y muestra cómo descargar Tailscale

### Requirement: Unión etiquetada a la tailnet
Un nodo sin sesión de Tailscale SHALL unirse a la tailnet sin login interactivo, con un nombre de host fijo y las etiquetas de la app (`tag:teco`, `tag:zotz` o ambas), usando el secreto de OAuth client de Tailscale.

#### Scenario: MSI con dos apps
- **WHEN** la MSI se instala para Teco y para Zotz
- **THEN** aparece en la tailnet con su nombre fijo y las etiquetas `tag:teco` y `tag:zotz`

### Requirement: Respeto de la sesión de usuario
Si la máquina ya tiene sesión de Tailscale con una cuenta de usuario, el instalador MUST NOT cerrar esa sesión, cambiar su cuenta ni agregarle etiquetas.

#### Scenario: Mac de trabajo
- **WHEN** se ejecuta `install.sh` en la Mac donde Hugo ya inició sesión en Tailscale
- **THEN** la Mac sigue a nombre de Hugo y el instalador solo configura Teco y `tailscale serve`

### Requirement: Secreto fuera del repositorio
El secreto de Tailscale SHALL leerse de la variable de entorno `TS_OAUTH_SECRET` o de un archivo local del usuario. El secreto MUST NOT versionarse, imprimirse ni escribirse en logs. Si falta y el nodo no está unido, el instalador SHALL fallar explicando dónde ponerlo.

#### Scenario: Falta el secreto
- **WHEN** se ejecuta el instalador en un nodo nuevo sin `TS_OAUTH_SECRET` ni archivo de secreto
- **THEN** termina con error, sin unir el nodo, e indica la ruta donde guardar el secreto

### Requirement: Panel solo por la tailnet
El instalador SHALL dejar Teco escuchando únicamente en `127.0.0.1` y publicarlo dentro de la tailnet por HTTPS con un nombre fijo mediante `tailscale serve`, sin exponerlo a internet.

#### Scenario: Acceso desde fuera de la tailnet
- **WHEN** un equipo que no está en la tailnet intenta abrir el panel del nodo
- **THEN** no puede llegar a él

### Requirement: Configuración predeterminada idéntica
El instalador SHALL escribir la misma configuración predeterminada en todos los nodos cuando no exista una, y MUST NOT sobrescribir una configuración existente.

#### Scenario: Configuración editada a mano
- **WHEN** `teco.toml` ya existe con reglas personalizadas y se vuelve a ejecutar el instalador
- **THEN** las reglas personalizadas se conservan

### Requirement: Arranque automático
El instalador SHALL registrar Teco para arrancar al iniciar sesión y reiniciarse si se cae: LaunchAgent `dev.hugorodriguez.teco` en macOS y tarea programada "Teco" en Windows.

#### Scenario: Teco se cae
- **WHEN** el proceso de Teco termina inesperadamente
- **THEN** el sistema lo vuelve a arrancar en menos de un minuto

### Requirement: Reutilizable por Zotz
Los instaladores SHALL parametrizar el nombre de la app, el binario, el puerto local, el puerto HTTPS y las etiquetas, de modo que Zotz los use sin cambiar su lógica.

#### Scenario: Instalar Zotz con el mismo script
- **WHEN** se ejecuta el instalador con los parámetros de Zotz
- **THEN** Zotz queda instalado y publicado en su propio puerto HTTPS del mismo nodo, sin afectar a Teco
