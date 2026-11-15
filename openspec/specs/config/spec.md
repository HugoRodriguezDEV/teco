# config Specification

## Purpose
Definir cómo se configura Teco: dónde vive `teco.toml`, qué valores trae por defecto, cómo se valida y cómo se recarga en caliente sin reiniciar el proceso.

## Requirements

### Requirement: Ubicación de la configuración
Teco SHALL usar `--config <ruta>` si se indica. Si no, SHALL usar `teco.toml` en la carpeta del sistema: `~/Library/Application Support/teco` en macOS, `%APPDATA%\teco` en Windows y `$XDG_CONFIG_HOME/teco` (o `~/.config/teco`) en otros sistemas.

#### Scenario: Ruta explícita
- **WHEN** se ejecuta `teco run --config ./dev.toml`
- **THEN** Teco usa `./dev.toml` y escribe su log en la misma carpeta

### Requirement: Configuración por defecto
Si el archivo no existe, Teco SHALL crearlo con los valores por defecto (panel en `127.0.0.1:7777`, muestreo cada 2 s, terminar desde el panel desactivado, lista `protected` de procesos del sistema, límites de sistema y las reglas "Proceso gigante", "Vitest colgado" y "CPU pegada al 100") y usarlo.

#### Scenario: Primera ejecución
- **WHEN** Teco arranca y no existe `teco.toml`
- **THEN** se crea el archivo con los valores por defecto y Teco vigila con ellos

### Requirement: Validación estricta
Teco SHALL rechazar una configuración con campos desconocidos, con más de 64 reglas o con alguna regla sin límites, indicando el error. `teco check` SHALL validar el archivo y listar las reglas sin arrancar el vigilante.

#### Scenario: Campo mal escrito
- **WHEN** `teco.toml` contiene `intervalo = 3`
- **THEN** la configuración se rechaza con un error que menciona el campo desconocido

### Requirement: Recarga en caliente
Teco SHALL detectar cambios en el archivo de configuración en cada muestra y aplicarlos sin reiniciar. Si el archivo nuevo es inválido, SHALL conservar la configuración anterior y registrar un evento de error. Un cambio de `bind` SHALL aplicarse solo al reiniciar.

#### Scenario: Archivo con error
- **WHEN** se guarda `teco.toml` con un error de sintaxis
- **THEN** Teco sigue con la configuración anterior y el error aparece en el historial

#### Scenario: Nueva regla
- **WHEN** se agrega una regla y se guarda el archivo
- **THEN** la regla empieza a evaluarse en la siguiente muestra y se registra "Config recargada"

### Requirement: Ruta visible
`teco config-path` SHALL imprimir la ruta del archivo de configuración que se usaría.

#### Scenario: Consultar la ruta
- **WHEN** se ejecuta `teco config-path` en macOS
- **THEN** imprime `~/Library/Application Support/teco/teco.toml` expandido
