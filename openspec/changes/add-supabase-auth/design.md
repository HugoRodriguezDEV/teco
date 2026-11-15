# Design

## Context

El panel se sirve por `tailscale serve` en un nombre HTTPS fijo (ver `add-node-bootstrap`) y Teco escucha solo en loopback, así que toda petición llega desde `127.0.0.1`. Para distinguir "localhost de verdad" de "vía tailscale serve" se usa **el puerto**, no la cabecera `Host` (que la escribe el cliente y se puede falsificar): Teco abre dos listeners en loopback, `bind` (local, por defecto `127.0.0.1:7777`) y `bind_tailnet` (por defecto `127.0.0.1:7778`), y `tailscale serve` apunta solo a `bind_tailnet`. Presupuesto del proyecto en `openspec/config.yaml`.

## Goals / Non-Goals

**Goals:**
- Un solo modelo de identidad para Teco y Zotz.
- Verificación local y barata en cada petición.
- Seguir funcionando sin internet desde la propia máquina.

**Non-Goals:**
- Login con correo y contraseña u otros proveedores.
- Integración con el auth de Qué Onda.
- Panel central de nodos (se habilita con `devices`, pero se construye después).
- Exponer el panel fuera de la tailnet.

## Decisions

**Supabase Auth con Google y llaves asimétricas.** Supabase publica las llaves públicas en `<proyecto>/auth/v1/.well-known/jwks.json`. Teco descarga el JWKS al arrancar y lo refresca cada 6 h o cuando aparece un `kid` desconocido (máximo una vez por minuto). Alternativa descartada: validar contra `/auth/v1/user` en cada petición (latencia y dependencia de internet).

**Roles dentro del token.** Tabla `app_access(email, app, role)` y un Custom Access Token Hook que agrega `app_roles: {"teco": "admin", "zotz": "viewer"}` al JWT. Si el correo no tiene filas, el hook no agrega roles y ninguna app lo deja pasar. Alternativa descartada: consultar la tabla desde cada app (requiere la llave de servicio en los nodos).

**Validación en Teco.** Algoritmos asimétricos únicamente (ES256/RS256); se rechaza `alg` simétrico o `none`. Se validan `iss` (URL del proyecto + `/auth/v1`), `aud = authenticated`, `exp` con 60 s de tolerancia y `app_roles.teco`. Resultado en caché por token hasta su `exp` (mapa acotado) para no re-verificar en cada polling.

**Sesión del panel.** Flujo PKCE manual: el panel genera `code_verifier`, redirige a `/auth/v1/authorize?provider=google&redirect_to=<panel>&code_challenge=...` y al volver canjea el código en `/auth/v1/token?grant_type=pkce` con la llave anon. Guarda access y refresh token en `sessionStorage` y envía `Authorization: Bearer` en cada fetch; renueva con el refresh token antes de expirar. Alternativa descartada: cookie HttpOnly emitida por Teco (más estado en el servidor, sin beneficio real dentro de la tailnet).

**Respaldo de localhost.** Las peticiones que llegan por el listener `bind` (local) se tratan como `admin` sin token; las que llegan por `bind_tailnet` SIEMPRE exigen token, sin importar su cabecera `Host`. Así un usuario de la tailnet no puede hacerse pasar por local mandando `Host: localhost`. Riesgo aceptado: quien ya tiene sesión en la máquina ya puede terminar procesos por otros medios.

**`teco login` y `devices`.** `teco login` levanta un puerto efímero en loopback, abre el navegador con PKCE y recibe el código ahí. Con el access token inserta o actualiza su fila en `devices(id, user_id, app, hostname, tailnet_name, os, version, last_seen)` con RLS `user_id = auth.uid()`. Guarda el refresh token en la carpeta de config con permisos de usuario y actualiza `last_seen` como máximo cada hora. Nunca guarda la llave de servicio.

**Configuración.**
```toml
[auth]
supabase_url = "https://<proyecto>.supabase.co"
anon_key = "<llave anon pública>"
app = "teco"
```
Sin sección `[auth]`, Teco se comporta como hoy.

**Costo.** Cliente HTTPS y verificación de JWT agregan ~2 MB al binario; el JWKS en memoria pesa unos KB. El refresco cada 6 h no afecta el CPU promedio.

## Risks / Trade-offs

- [Sin internet al arrancar no hay JWKS] → se guarda una copia del JWKS en disco y se usa si la descarga falla; localhost siempre funciona.
- [Un rol revocado sigue válido hasta que expira el token] → vida del access token de 1 h (default de Supabase); aceptado.
- [Llave anon y URL en `teco.toml`] → son públicas por diseño; las protecciones reales son RLS y la lista de acceso.
- [Confundir tráfico de la tailnet con local] → la decisión es por listener (puerto), nunca por `Host`; `tailscale serve` solo conoce `bind_tailnet`. Si `bind` y `bind_tailnet` coinciden, Teco se niega a arrancar con `[auth]` activo.

## Migration Plan

1. Crear el proyecto de Supabase, tablas, RLS, hook y proveedor Google (con el conector de Supabase).
2. Registrar las URLs `https://<nodo>.<tailnet>.ts.net` como URLs de regreso.
3. Desplegar Teco con `[auth]`; verificar login y roles desde la Mac.
4. Rollback: quitar la sección `[auth]` de `teco.toml` (recarga en caliente) y Teco vuelve al comportamiento anterior.

## Open Questions

- Nombre del proyecto de Supabase y región: no cambian el diseño.
