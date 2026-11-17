# Tasks

- [x] 1. `GpuProbe` y `GpuSnapshot`: detección de tarjetas reales, lectura en el hilo del muestreo; verificar `cargo check` para Windows
- [x] 2. Reglas `max_gpu_percent`/`max_vram_percent` (sistema) y `max_gpu_percent`/`max_vram_gb` (proceso), con prueba unitaria de VRAM
- [x] 3. JSON sin `gpu` cuando no hay tarjeta (prueba unitaria) y con `gpu`/`cards` cuando sí
- [x] 4. Panel y tablero: barras, columna y orden por GPU solo con tarjeta; verificar en navegador ambos casos
- [ ] 5. Probar en la MSI: valores parecidos a los del Administrador de tareas y costo con `teco bench`
