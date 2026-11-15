# Tasks

## 1. Proyecto de Supabase (con el conector de Supabase)

- [ ] 1.1 Crear el proyecto dedicado y desactivar registro con correo y contraseña; verificar en Auth → Providers que solo Google está activo
- [ ] 1.2 Configurar el proveedor Google con las credenciales OAuth que Hugo crea en Google Cloud Console; verificar iniciando sesión de prueba desde el panel de Supabase
- [ ] 1.3 Crear la tabla `app_access(email, app, role)` con RLS que bloquea anon y usuarios, y la fila de Hugo como `admin` en `teco` y `zotz`; verificar que un `insert` con token de usuario falla
- [ ] 1.4 Crear el Custom Access Token Hook que agrega `app_roles` al JWT; verificar decodificando un token de Hugo y viendo ambos roles
- [ ] 1.5 Activar llaves de firma asimétricas y confirmar que `/auth/v1/.well-known/jwks.json` publica la llave; verificar con `curl`
- [ ] 1.6 Crear la tabla `devices` con RLS `user_id = auth.uid()` y clave única por (user_id, app, hostname); verificar que otra cuenta no ve filas ajenas
- [ ] 1.7 Registrar como URLs de regreso los nombres `https://<nodo>.<tailnet>.ts.net` y `http://127.0.0.1:*` para `teco login`; verificar en Auth → URL Configuration

## 2. Verificación de tokens en Teco

- [ ] 2.1 Agregar la sección opcional `[auth]` a la configuración con validación y recarga en caliente; verificar con pruebas de `config` (sin sección, sección válida, campo faltante)
- [ ] 2.2 Implementar descarga, caché en memoria y copia en disco del JWKS con refresco cada 6 h y ante `kid` desconocido (máximo 1/min); verificar con pruebas que simulan arranque sin red usando la copia local
- [ ] 2.3 Implementar la validación de JWT (solo algoritmos asimétricos, `iss`, `aud`, `exp` con 60 s, rol de la app) con caché por token; verificar con pruebas usando llaves de prueba: válido, HS256, `none`, expirado, sin rol, otro emisor
- [ ] 2.4 Medir el impacto con `teco bench` y verificar que el binario queda bajo 6 MB y RSS/CPU dentro del presupuesto

## 3. Panel

- [ ] 3.1 Exigir `viewer` en `/api/state` y `admin` en `/api/kill` con autenticación activa, con excepción solo para el listener local `bind` (nunca por `Host`), agregar `bind_tailnet`, y registrar el correo en eventos de terminación; verificar con pruebas de servidor (401, 403, 200 por rol, desde `bind`, y 401 con `Host: localhost` falso por `bind_tailnet`)
- [ ] 3.2 Implementar en `index.html` el flujo PKCE (Entrar con Google, canje del código, `sessionStorage`, renovación, salir) sin recursos externos; verificar con Playwright contra un emisor de prueba y revisando que la CSP no se relaja salvo `connect-src` al proyecto
- [ ] 3.3 Ocultar el botón Terminar para `viewer` y mostrar acceso denegado sin rol; verificar con capturas por rol
- [ ] 3.4 Documentar la sección `[auth]` y el flujo en `README.md`; verificar siguiendo la guía en la Mac

## 4. teco login y dispositivos

- [ ] 4.1 Implementar `teco login` con puerto efímero en loopback, PKCE y guardado de la sesión con permisos de usuario; verificar iniciando sesión en la Mac
- [ ] 4.2 Registrar o actualizar el dispositivo y refrescar `last_seen` como máximo cada hora; verificar en Supabase que un segundo login no duplica filas
- [ ] 4.3 Documentar `teco login` en `README.md`; verificar ejecutándolo en la MSI siguiendo el README

## 5. Integración

- [ ] 5.1 Probar de punta a punta en la tailnet: Hugo `admin` termina un proceso desde la Mac, una cuenta `viewer` solo ve, una cuenta sin fila recibe 403, y `http://127.0.0.1:7777` en la MSI funciona sin internet; verificar que quitar `[auth]` restaura el comportamiento anterior
