# Spec Delta

## Purpose
Definir el modelo de identidad compartido por las herramientas internas (Teco, Zotz) en un proyecto propio de Supabase: quién puede entrar, a qué app y con qué rol, independiente del auth de Qué Onda.

## ADDED Requirements

### Requirement: Proyecto de identidad propio
Las herramientas internas SHALL usar un proyecto de Supabase dedicado, separado de Qué Onda, con Google como único proveedor de inicio de sesión.

#### Scenario: Intento con correo y contraseña
- **WHEN** alguien intenta registrarse con correo y contraseña
- **THEN** el proyecto no lo permite

### Requirement: Lista de acceso por correo, app y rol
El acceso SHALL definirse en una lista de filas (correo, app, rol) con roles `viewer` y `admin`. Un correo sin fila para una app MUST NOT obtener acceso a esa app aunque inicie sesión con Google.

#### Scenario: Agregar a alguien del equipo
- **WHEN** se agrega la fila (`persona@equipo.com`, `zotz`, `viewer`)
- **THEN** esa persona puede ver Zotz en su siguiente inicio de sesión y sigue sin acceso a Teco

### Requirement: Roles dentro del token
Al emitir un token, el proyecto SHALL incluir un mapa de roles por app según la lista de acceso, para que cada app decida sin consultar la base de datos.

#### Scenario: Token de Hugo
- **WHEN** Hugo, con rol `admin` en `teco` y en `zotz`, inicia sesión
- **THEN** su token incluye ambos roles

### Requirement: Lista de acceso protegida
La lista de acceso MUST NOT ser legible ni modificable con la llave anon o con el token de un usuario normal; solo se administra desde el panel de Supabase o con la llave de servicio fuera de los nodos.

#### Scenario: Usuario intenta darse permisos
- **WHEN** un usuario autenticado intenta insertar una fila en la lista de acceso con su token
- **THEN** la base de datos lo rechaza
