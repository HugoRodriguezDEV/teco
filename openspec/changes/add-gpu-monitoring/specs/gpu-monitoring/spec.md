# Spec Delta

## Purpose
Vigilar la tarjeta gráfica cuando la máquina tiene una, sin mostrar nada de GPU cuando no la tiene.

## ADDED Requirements

### Requirement: Solo si hay tarjeta
Teco SHALL detectar al arrancar si hay al menos una tarjeta gráfica real; los adaptadores de software y de máquinas virtuales MUST NOT contar. Sin tarjeta, Teco MUST NOT medir la GPU ni incluir datos de GPU en su JSON, y el panel y el tablero MUST NOT mostrar nada de GPU.

#### Scenario: Mac sin tarjeta dedicada en una VM
- **WHEN** Teco corre en una máquina cuyo único adaptador es "Microsoft Basic Render Driver"
- **THEN** el panel no muestra barras, columnas ni botones de GPU

### Requirement: Medición de la tarjeta
Con tarjeta, Teco SHALL reportar el uso de la GPU más cargada, la memoria de video usada y total (suma de las tarjetas que la reportan) y, por tarjeta, su nombre, uso y memoria. Donde el sistema lo dé (Windows), SHALL reportar el uso de GPU y la memoria de GPU de cada proceso.

#### Scenario: MSI con NVIDIA
- **WHEN** un proceso usa 6 GB de VRAM en la MSI
- **THEN** el panel muestra la barra VRAM y la columna GPU de ese proceso

### Requirement: Reglas de GPU
`[system]` SHALL aceptar `max_gpu_percent` y `max_vram_percent`, y cada `[[rule]]` SHALL aceptar `max_gpu_percent` y `max_vram_gb`. Siguen la misma semántica de `for_secs`, aviso único, `cooldown_min`, `include_children` y botonera que las demás métricas.

#### Scenario: Modelo que se come la VRAM
- **WHEN** una regla con `max_vram_gb = 6` y `for_secs = 60` ve un proceso arriba de 6 GB durante 60 s
- **THEN** se dispara la alerta y el tablero ofrece "Detener" y "Matar" para ese proceso
