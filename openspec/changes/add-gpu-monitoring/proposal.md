# Proposal

## Why

La MSI tiene tarjeta gráfica y los agentes pueden usarla (por ejemplo, modelos de IA locales). Hoy Teco no ve la GPU: un proceso puede llenar la memoria de video sin que nadie se entere. Hugo pidió vigilarla, con una condición: **en máquinas sin tarjeta no debe aparecer nada de GPU**.

## What Changes

- Teco detecta al arrancar si la máquina tiene una tarjeta gráfica **real**. Descarta adaptadores de software y de máquinas virtuales (Microsoft Basic Render, llvmpipe, VMware SVGA, etc.). Si no hay ninguna, no mide nada de GPU y su JSON no trae datos de GPU.
- Con tarjeta, mide:
  - el uso de la GPU (la tarjeta más cargada) y la memoria de video usada contra la total (suma de las tarjetas);
  - el uso de GPU y la memoria de GPU de cada proceso (Windows).
- Reglas nuevas:
  - en `[system]`: `max_gpu_percent` y `max_vram_percent`;
  - por regla: `max_gpu_percent` y `max_vram_gb`.
  - Las alertas y la botonera funcionan igual que con CPU y RAM.
- Panel y tablero: barras GPU y VRAM, columna GPU y orden por GPU, solo si hay tarjeta.
- Configuración por defecto: `max_vram_percent = 95`, que solo actúa si hay tarjeta.
- La tarjeta la lee solo el hilo del muestreo (`GpuProbe`): en Windows y Linux, `sysinfo::Gpus` guarda handles nativos que no deben cruzar hilos. El estado compartido recibe un `GpuSnapshot` con datos simples.

## Capabilities

### New Capabilities
- `gpu-monitoring`: medición de la tarjeta gráfica y reglas sobre ella, visibles solo si existe.

## Impact

- `sysinfo` con la feature `gpu`. El binario de Linux pasa de 1.0 a 1.26 MB.
- Sin tarjeta, el costo es cero: no se crea el contador de GPU por proceso.
- Pruebas: la CI no tiene GPU real, así que la primera prueba real es en la MSI.
