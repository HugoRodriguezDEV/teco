# bench Specification

## Purpose
Comprobar con datos que el monitor nunca es el problema: medir el consumo propio de Teco en condiciones reales y compararlo contra el presupuesto del proyecto.

## Requirements

### Requirement: Comando bench
`teco bench` SHALL muestrear durante `--secs` segundos (por defecto 60) con la configuración normal y con el panel escuchando en un puerto efímero sin visitantes, imprimiendo el progreso cada 10 s.

#### Scenario: Corrida corta
- **WHEN** se ejecuta `teco bench --secs 20`
- **THEN** el comando muestrea 20 s e imprime el resultado

### Requirement: Sin efectos durante la medición
Durante `teco bench`, Teco MUST NOT enviar notificaciones, webhooks ni terminar procesos, ni escribir en `teco-events.log`.

#### Scenario: Regla de terminar activa
- **WHEN** una regla con `action = "kill"` se dispararía durante el bench
- **THEN** ningún proceso se termina

### Requirement: Reporte contra el presupuesto
Al terminar, `teco bench` SHALL reportar RSS final y pico (límite 15 MB), CPU promedio propia (límite 0.5 % de un núcleo), tamaño del binario (límite 6 MB) y tiempo hasta la primera muestra (límite 200 ms), marcando cada uno como dentro o fuera del límite.

#### Scenario: Build de depuración
- **WHEN** se ejecuta `teco bench` con un build de depuración
- **THEN** el reporte advierte que hay que medir con el build release
