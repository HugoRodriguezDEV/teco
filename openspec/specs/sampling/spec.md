# sampling Specification

## Purpose
Tomar, cada `interval_secs`, una foto barata del equipo (CPU, RAM, swap) y de cada proceso (PID, padre, nombre, comando, CPU, RAM, tiempo vivo) para que el motor de reglas y el panel trabajen sobre ella.

## Requirements

### Requirement: Muestreo periódico
Teco SHALL tomar una muestra cada `interval_secs` segundos (por defecto 2, acotado entre 1 y 3600). Si el proceso se atrasa (por ejemplo, tras suspender el equipo), SHALL reprogramar la siguiente muestra a partir del momento actual en vez de tomar varias seguidas.

#### Scenario: Intervalo por defecto
- **WHEN** `teco.toml` no define `interval_secs`
- **THEN** Teco toma una muestra cada 2 segundos

#### Scenario: Recuperación tras un atraso
- **WHEN** el equipo despierta tras estar suspendido varios minutos
- **THEN** Teco toma una sola muestra y retoma el ritmo normal sin ráfagas

### Requirement: Métricas globales
Cada muestra SHALL incluir el uso de CPU global en porcentaje, el número de núcleos, la RAM usada y total, y la swap usada y total.

#### Scenario: Equipo sin swap
- **WHEN** el equipo no tiene swap configurada
- **THEN** la muestra reporta swap total 0 y el panel muestra "sin swap"

### Requirement: Métricas por proceso
Cada muestra SHALL incluir por proceso: PID, PID del padre, hora de inicio, nombre, línea de comando truncada a 300 caracteres, CPU en porcentaje de un núcleo (puede pasar de 100), RAM residente y tiempo vivo en segundos.

#### Scenario: Proceso multihilo
- **WHEN** un proceso usa cuatro núcleos al máximo
- **THEN** su CPU reportada es cercana a 400 %

### Requirement: Solo procesos, no hilos
Teco SHALL excluir los hilos de la lista de procesos en las plataformas donde el sistema los reporta como entradas separadas.

#### Scenario: Linux lista hilos
- **WHEN** el sistema operativo reporta los hilos de un proceso como entradas propias
- **THEN** solo aparece el proceso, una vez

### Requirement: Identidad estable de cada proceso
Un proceso SHALL identificarse por su PID junto con su hora de inicio. Si un PID reaparece con otra hora de inicio, Teco SHALL tratarlo como un proceso nuevo y volver a leer su nombre y comando.

#### Scenario: PID reciclado
- **WHEN** un proceso termina y el sistema asigna su PID a otro proceso
- **THEN** Teco no hereda el nombre, el comando ni el estado de alertas del proceso anterior

### Requirement: Costo mínimo por muestra
Teco SHALL refrescar únicamente CPU, memoria y, una sola vez por proceso, la línea de comando. Nombre y comando de un proceso ya visto MUST NOT volver a leerse del sistema en muestras posteriores.

#### Scenario: Proceso de larga vida
- **WHEN** un proceso lleva horas vivo
- **THEN** su línea de comando se leyó una sola vez, en la primera muestra en que apareció
