# Spec Delta

## Purpose
Registrar automáticamente cada máquina donde corre una herramienta interna a nombre de la cuenta que inicia sesión, para saber qué nodos existen sin darlos de alta a mano.

## ADDED Requirements

### Requirement: Comando teco login
`teco login` SHALL abrir el navegador para iniciar sesión con Google, recibir el resultado en un puerto local temporal y guardar la sesión del nodo en la carpeta de configuración con permisos solo del usuario.

#### Scenario: Primer login en la MSI
- **WHEN** Hugo ejecuta `teco login` en la MSI e inicia sesión en el navegador
- **THEN** la terminal confirma el inicio de sesión y el nodo queda registrado

### Requirement: Registro del dispositivo
Tras iniciar sesión, el nodo SHALL crear o actualizar su registro con app, nombre de host, nombre en la tailnet, sistema operativo, versión y última conexión, ligado a la cuenta. Repetir el login MUST NOT crear duplicados.

#### Scenario: Login repetido
- **WHEN** se ejecuta `teco login` otra vez en la misma máquina
- **THEN** se actualiza el registro existente en vez de crear otro

### Requirement: Aislamiento entre cuentas
Cada cuenta SHALL ver y modificar solo sus propios dispositivos.

#### Scenario: Otra cuenta consulta dispositivos
- **WHEN** una cuenta distinta a la de Hugo consulta la tabla de dispositivos
- **THEN** no ve los nodos de Hugo

### Requirement: Última conexión
Un nodo con sesión SHALL actualizar su última conexión como máximo una vez por hora mientras corre.

#### Scenario: Nodo encendido todo el día
- **WHEN** la MSI corre 24 horas seguidas
- **THEN** su última conexión se actualiza alrededor de 24 veces, no en cada muestra
