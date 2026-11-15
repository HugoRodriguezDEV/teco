# Proposal

## Why

La red ya es privada (Tailscale), pero dentro de ella hoy cualquiera que llegue al panel puede ver los procesos y, si está activado, terminarlos. Hugo quiere un login real para sus herramientas internas, compartido por Teco y Zotz, que permita sumar después a gente del equipo por correo y con roles, sin depender de Qué Onda (que tiene su propio auth en un VPS con Neon). Se hace ya porque es el "brinco" que van a dar ambas herramientas y conviene definir un solo modelo de identidad desde el inicio.

**Depende de `add-node-bootstrap`**: el login con Google necesita que cada panel tenga un nombre HTTPS fijo (`https://<host>.tail59ed16.ts.net`) registrado como URL de regreso en Supabase.

## What Changes

- Proyecto nuevo de Supabase solo para las herramientas internas (Teco, Zotz), independiente de Qué Onda.
- Login únicamente con Google (OAuth) vía Supabase Auth.
- Tabla de acceso por correo, app y rol (`viewer`, `admin`); un correo sin fila no entra a ninguna app.
- Un hook de token de Supabase agrega al JWT los roles por app; Supabase firma con llaves asimétricas.
- Teco verifica el JWT localmente con las llaves públicas (JWKS) en caché: firma, emisor, audiencia, expiración y rol para la app `teco`. Sin llamadas a Supabase por petición.
- `viewer` puede ver el panel; `admin` además puede terminar procesos. **BREAKING**: con `[auth]` activado, `/api/state` y `/api/kill` exigen sesión desde la tailnet.
- Peticiones desde `localhost` siguen funcionando sin login, como respaldo si no hay internet.
- Inicio de sesión en el panel con flujo PKCE en JS vanilla, sin `supabase-js` ni CDN.
- `teco login`: abre el navegador, inicia sesión y registra la máquina en una tabla `devices` ligada a la cuenta.

## Capabilities

### New Capabilities
- `panel-auth`: inicio de sesión en el panel, verificación local de tokens, roles por app y respaldo de localhost.
- `access-control`: modelo de identidad compartido en Supabase (lista de acceso por correo, app y rol, roles dentro del token, aislamiento entre cuentas).
- `device-registration`: registro de nodos con `teco login` ligados a la cuenta del usuario.

### Modified Capabilities
- `panel`: `/api/state` y `/api/kill` exigen el rol correspondiente cuando la autenticación está activada; el botón Terminar depende del rol `admin`.

## Impact

- Código: nuevo módulo de autenticación en Teco, sección `[auth]` en `teco.toml`, cambios en el servidor del panel y en `index.html`, nuevo subcomando `teco login`.
- Dependencias nuevas en Rust: verificación de JWT con llaves asimétricas y un cliente HTTPS para descargar las llaves. Binario estimado ~3 MB (sigue bajo el límite de 6 MB); RSS y CPU en reposo sin cambio relevante.
- Supabase: proyecto, proveedor Google, tablas `app_access` y `devices` con RLS, hook de token. Credenciales OAuth de Google creadas en Google Cloud Console por Hugo.
- Secretos: solo la URL del proyecto y la llave anon (públicas) van en la configuración; ningún secreto en el repositorio.
- Zotz: usará el mismo proyecto y el mismo modelo de roles con `app = "zotz"` en su propio repositorio.
