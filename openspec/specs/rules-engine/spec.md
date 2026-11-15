# rules-engine Specification

## Purpose
Decidir, a partir de cada muestra, cuándo algo "se pasó de la raya": evaluar las reglas de `teco.toml` y los límites del sistema, evitar falsas alarmas por picos, avisar una sola vez y, si la regla lo pide, terminar procesos de forma segura.

## Requirements

### Requirement: Coincidencia de reglas
Una regla con `match = "*"` (o vacío) SHALL aplicar a todos los procesos. Con cualquier otro valor SHALL aplicar a los procesos cuyo nombre o línea de comando contengan ese texto, sin distinguir mayúsculas.

#### Scenario: Coincidencia por comando
- **WHEN** existe la regla `match = "vitest"` y corre `bun.exe` con el comando `bun x vitest run`
- **THEN** la regla aplica a ese proceso

### Requirement: Límites de una regla
Una regla SHALL tener al menos uno de `max_mem_gb`, `max_cpu_percent` o `max_runtime_min`. Un proceso pasa el límite cuando su valor es mayor o igual al límite. Si hay varios límites, se reporta el de mayor proporción valor/límite.

#### Scenario: Varios límites
- **WHEN** una regla tiene `max_mem_gb = 4` y `max_runtime_min = 40`, y el proceso lleva 43 min con 3.1 GB
- **THEN** la regla reporta el tiempo vivo: 43 min contra el límite de 40

### Requirement: Condición sostenida
Una regla SHALL dispararse solo cuando el límite se sostiene durante `for_secs` segundos seguidos (por defecto 0, que dispara en la primera muestra arriba). Si el valor baja del límite antes, el conteo se reinicia.

#### Scenario: Pico corto
- **WHEN** un proceso pasa el límite 30 s y la regla tiene `for_secs = 60`
- **THEN** la regla no se dispara

#### Scenario: Pico sostenido
- **WHEN** el proceso se mantiene arriba del límite durante 60 s seguidos
- **THEN** la regla se dispara una vez

### Requirement: Un aviso por regla y proceso
Teco SHALL avisar una sola vez por cada par (regla, proceso). Si el proceso sigue arriba, SHALL volver a avisar cada `cooldown_min` minutos (por defecto 15; 0 desactiva la repetición).

#### Scenario: Proceso que sigue arriba
- **WHEN** una regla ya se disparó y el proceso sigue arriba 15 minutos después
- **THEN** Teco vuelve a avisar indicando que sigue así

### Requirement: Re-armado y resolución
Una regla disparada SHALL re-armarse cuando el proceso se mantiene por debajo del límite durante `for_secs`, o cuando el proceso termina. En ambos casos SHALL registrarse un evento de resolución.

#### Scenario: Proceso que termina
- **WHEN** un proceso con una regla disparada termina
- **THEN** se registra un evento "terminó" y la alerta deja de estar activa

### Requirement: Agrupación por árbol de procesos
Con `include_children = true`, cada proceso SHALL contarse dentro de su ancestro más alto que coincide con la regla, y la regla SHALL evaluar la suma de RAM y CPU del grupo. El tiempo vivo usado es el de la raíz.

#### Scenario: Hijo que también coincide
- **WHEN** `bun` (vitest) tiene un worker que también contiene "vitest" en su comando, y ese worker tiene un hijo
- **THEN** los tres cuentan en un solo grupo cuya raíz es `bun`, sin contar el worker dos veces

### Requirement: Límites del sistema
La sección `[system]` SHALL vigilar la RAM total del equipo en porcentaje (`max_mem_percent`), la swap usada en GB (`max_swap_gb`) y la CPU global en porcentaje (`max_cpu_percent`), con su propio `for_secs` y `cooldown_min`, y la misma semántica de disparo que las reglas.

#### Scenario: RAM del equipo sostenida
- **WHEN** la RAM usada pasa el 90 % durante 60 s con `max_mem_percent = 90` y `for_secs = 60`
- **THEN** se dispara la alerta "RAM del equipo"

### Requirement: Estado general
Teco SHALL calcular un estado general en cada muestra: "alert" si hay alguna regla disparada; si no, "watch" si algo está al 80 % o más de su límite; si no, "ok".

#### Scenario: Cerca del límite
- **WHEN** un proceso usa 3.3 GB con una regla de `max_mem_gb = 4` y nada está disparado
- **THEN** el estado general es "watch"

### Requirement: Acción terminar
Con `action = "kill"`, Teco SHALL primero avisar y registrar el evento, y después terminar el proceso (o todo su grupo con `include_children`), saltando los que estén protegidos.

#### Scenario: Regla de terminar
- **WHEN** se dispara una regla con `action = "kill"`
- **THEN** la notificación y el evento existen antes de que el proceso reciba la señal de terminar

### Requirement: Procesos que nunca se terminan
Teco MUST NOT terminar: su propio proceso, los PID 0, 1 y 4, ni ningún proceso cuyo nombre esté en `protected` (comparación exacta sin distinguir mayúsculas). Tampoco MUST terminar un PID cuya hora de inicio ya no coincide.

#### Scenario: Proceso protegido
- **WHEN** una regla de terminar coincide con `lsass.exe` y `lsass.exe` está en `protected`
- **THEN** el proceso no se termina y se registra un evento de error explicando por qué

### Requirement: Secuencia de terminación
En Unix, Teco SHALL enviar SIGTERM y, si el mismo proceso sigue vivo 5 s después, SIGKILL. En Windows SHALL usar TerminateProcess.

#### Scenario: Proceso que ignora SIGTERM
- **WHEN** un proceso en macOS sigue vivo 5 s después de recibir SIGTERM
- **THEN** Teco le envía SIGKILL y registra el evento
