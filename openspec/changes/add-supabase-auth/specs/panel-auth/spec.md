# Spec Delta

## Purpose
Controlar quién puede ver el panel de Teco y quién puede terminar procesos, mediante inicio de sesión con Google vía Supabase y verificación local de tokens, sin perder el acceso local cuando no hay internet.

## ADDED Requirements

### Requirement: Autenticación opcional por configuración
Teco SHALL activar la autenticación solo cuando `teco.toml` tiene una sección `[auth]` con `supabase_url`, `anon_key` y `app`. Sin esa sección, el panel SHALL comportarse como antes.

#### Scenario: Sin sección auth
- **WHEN** `teco.toml` no tiene `[auth]`
- **THEN** el panel funciona sin pedir login

#### Scenario: Activar con recarga en caliente
- **WHEN** se agrega la sección `[auth]` y se guarda el archivo
- **THEN** las siguientes peticiones desde la tailnet exigen sesión, sin reiniciar Teco

### Requirement: Inicio de sesión con Google
Cuando la autenticación está activa y no hay sesión, el panel SHALL ofrecer "Entrar con Google", iniciar el flujo de Supabase con PKCE y volver al mismo panel con la sesión iniciada, sin cargar recursos de terceros en la página.

#### Scenario: Primer acceso desde la Mac
- **WHEN** Hugo abre el panel de la MSI por su nombre `.ts.net` sin sesión
- **THEN** ve "Entrar con Google" y, tras iniciar sesión, ve el panel completo

### Requirement: Verificación local del token
Teco SHALL aceptar un token solo si su firma es válida con las llaves públicas del proyecto (algoritmo asimétrico), su emisor es el proyecto configurado, su audiencia es `authenticated` y no ha expirado. Teco MUST NOT consultar a Supabase en cada petición.

#### Scenario: Token firmado con algoritmo simétrico
- **WHEN** llega un token con `alg` HS256 o `none`
- **THEN** Teco responde 401

#### Scenario: Token expirado
- **WHEN** llega un token cuyo `exp` pasó hace más de 60 s
- **THEN** Teco responde 401 y el panel intenta renovar la sesión

### Requirement: Roles por app
Teco SHALL leer el rol del usuario para la app configurada desde el token: `viewer` permite ver el panel y `admin` además permite terminar procesos. Un token válido sin rol para la app SHALL recibir 403.

#### Scenario: Usuario sin acceso a Teco
- **WHEN** inicia sesión un correo que solo tiene rol en Zotz
- **THEN** el panel de Teco responde 403 con un mensaje de acceso denegado

### Requirement: Respaldo local
Con autenticación activa Teco SHALL escuchar en dos direcciones de loopback distintas: `bind` (local) y `bind_tailnet` (destino de `tailscale serve`). Las peticiones recibidas por `bind` SHALL atenderse sin token y con permisos de `admin`. Las recibidas por `bind_tailnet` MUST exigir token válido sin importar la cabecera `Host`. Teco MUST NOT arrancar si ambas direcciones coinciden.

#### Scenario: Sin internet en la MSI
- **WHEN** la MSI pierde internet y alguien abre `http://127.0.0.1:7777` en ella
- **THEN** el panel funciona completo sin pedir login

#### Scenario: Host falsificado desde la tailnet
- **WHEN** un dispositivo de la tailnet pide `/api/kill` por el nombre `.ts.net` enviando `Host: localhost` y sin token
- **THEN** la petición llega por `bind_tailnet` y Teco responde 401

### Requirement: Llaves públicas en caché
Teco SHALL descargar las llaves públicas al arrancar, refrescarlas periódicamente y ante un identificador de llave desconocido (como máximo una vez por minuto), y guardar una copia local para usarla si la descarga falla.

#### Scenario: Arranque sin internet
- **WHEN** Teco arranca sin internet y existe una copia local de las llaves
- **THEN** sigue aceptando tokens válidos firmados con esas llaves

### Requirement: Sin secretos en Teco
La configuración de Teco SHALL contener solo valores públicos del proyecto (URL y llave anon). Teco MUST NOT requerir ni aceptar la llave de servicio de Supabase.

#### Scenario: Revisar teco.toml
- **WHEN** se inspecciona un `teco.toml` con autenticación
- **THEN** solo contiene la URL del proyecto, la llave anon y el nombre de la app
