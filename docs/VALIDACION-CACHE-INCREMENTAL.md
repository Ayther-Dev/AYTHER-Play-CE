# Validación de la caché incremental

Estado: etapa de desarrollo; no equivale a la aceptación completa de `PLAN-CACHE-INCREMENTAL.md`.

Actualización del 2026-09-27: la prueba de intercambio de reservas falla contra beta.5 y pasa contra la distribución publicada beta.6. También pasan las otras dos pruebas de lanzamiento y la detección real de core: cuatro integraciones aprobadas. Las cinco comprobaciones de contrato CLI pasan contra las fuentes fijadas al commit publicado. Beta.6 conserva Engine rc.9, pasó 52/52 tests por plataforma en su publicación y ya está seleccionada en Play. [PR #24](https://github.com/Ayther-Dev/AYTHER-Runtime/pull/24); [evidencia y referencias](../../docs/dependency-normalization-2026-09-27.md). Esto no amplía las validaciones de caché descritas aquí.

## Comandos reproducibles

Ejecutar desde la raíz de Play CE, con Rust 1.95.0 y los checkouts hermanos de Play CE y Runtime:

```powershell
$env:AYTHER_RUNTIME_SOURCE = (Resolve-Path ../AYTHER-Runtime).Path
cargo fmt --package ayther_play_ce --check
cargo clippy --offline --all-targets --all-features --no-deps -- -D warnings
cargo test --offline
cargo test --offline -p ayther_core --lib
cargo test --offline -p ayther_core --features local-packs --lib
cargo build --release --offline --bin ayther_play_ce
```

La prueba del intercambio real entre procesos utiliza el Runtime compilado con los cambios de esta etapa:

```powershell
$env:AYTHER_RUNTIME_EXE = (Resolve-Path ../AYTHER-Runtime/out/build/rc9/bin/Release/ayther_runtime.exe).Path
cargo test --offline --bin ayther_play_ce runtime_accepts_play_revision -- --ignored --nocapture
```

La prueba envía tokens calculados por Play a Runtime y verifica aceptación/rechazo. Sale por un registro de confianza inexistente después de la confirmación, antes de inicializar SDL o un core. No necesita ROMs comerciales ni modifica guardados.

Contrato nativo de bloqueo de archivos:

```powershell
cmake -S tests/native -B artifacts/runtime-reservation/tracked-build -G "Visual Studio 18 2026" -A x64 -T v145
cmake --build artifacts/runtime-reservation/tracked-build --config Release --parallel 2
ctest --test-dir artifacts/runtime-reservation/tracked-build -C Release --output-on-failure
```

Se puede elegir otro generador MSVC y pasar `-DAYTHER_RUNTIME_SOURCE=<checkout>` si Runtime no está en la ubicación hermana.

## Cobertura añadida

- JSON inválido conserva su original y puede reintentarse; esquema futuro se rechaza sin downgrade. La importación verifica conteos, IDs y el contenido completo de cada registro; identidades o rutas duplicadas revierten toda la transacción.
- Escritores simultáneos se excluyen; corrupción no sustituye la DB por una biblioteca vacía; copias consistentes verificadas y rotación de tres versiones.
- Escrituras idénticas no vuelven a actualizar filas; sesiones repetidas no duplican tiempo jugado.
- Una edición de usuario tardía conserva cambios de DAT y estadísticas; quitar y volver a añadir una raíz recupera el historial del mismo juego.
- Metadatos de packs separados de auditoría, cancelación sin publicación, reutilización de inspecciones confirmadas y detección de cambios.
- Interrupción antes y después del renombrado de una instalación: original/staging preservados, recuperación idempotente y auditoría recuperada marcada pendiente.
- Reconciliación por directorio, raíz desconectada sin bajas y baja de descendientes recuperada después de persistir el inventario pero antes de actualizar los juegos.
- Cambio de ROM aborta la intención de lanzamiento y conserva el historial anterior.
- Índice de portadas sin nuevas búsquedas de disco; imágenes corruptas no impiden cargar otros juegos.
- Reservas de archivos bloquean escritura, borrado y renombrado; Runtime reconoce el token generado por Play y rechaza otro token.
- Un Runtime anterior que rechaza los argumentos de reserva produce un mensaje de actualización; un fallo de reserva anterior a la confirmación se distingue de los errores posteriores de firma/integridad.

Los tests de política de firma del core se ejecutan sin `local-packs`. Con esa feature, las dos pruebas que presuponen comprobación implícita de firma no se aplican; la suite CE de integridad y las llamadas explícitas a `TrustStore` conservan sus pruebas.

## Evidencia y límites

Resultados de esta ejecución (26 de septiembre de 2026):

| Comprobación | Resultado |
|---|---|
| Play CE, suite completa | 430 pruebas aprobadas, 6 ignoradas |
| Core, sin `local-packs` | 407 aprobadas, 1 ignorada |
| Core, con `local-packs` | 405 aprobadas, 1 ignorada |
| Intercambio real Play/Runtime de revisiones | 1 aprobada, ejecutada explícitamente |
| Contrato nativo de reservas, CTest | 1/1 aprobada |
| Formato y Clippy de Play, todos los targets/features con `-D warnings` | Aprobados |
| Compilaciones Release de Play CE y Runtime | Generadas correctamente |

La suite completa no ejecuta las seis pruebas ignoradas; la prueba del intercambio de revisiones se ejecutó por separado con el Runtime recién compilado. No se cuentan como aprobadas las restantes pruebas ignoradas.

Los logs de esta ejecución se guardan en `artifacts/cache-validation/`. Ese directorio y la compilación de vista previa están ignorados por Git. El código de las pruebas nativas está versionable en `tests/native/`.

Compilación disponible en `artifacts/cache-incremental-preview/ayther_play_ce.exe`, junto al Runtime compatible y sus dependencias. `LEEME.md` explica cómo seleccionar ese Runtime si las preferencias apuntan a otra versión. La copia del ejecutable de Play se verificó contra `target/release` mediante SHA-256; `SHA256SUMS.txt` registra las huellas del paquete. El intercambio de revisiones se probó también contra el Runtime de esa carpeta. No se abrió la GUI del paquete sobre la biblioteca real del usuario.

No se midieron aún las series de arranque frío/caliente ni los p95 de interacción de §10. No se declara verificada la interfaz visual, la continuidad del diario USN entre procesos, la paginación SQL, el cierre de Play durante una partida real ni la matriz completa de inyección de fallos. El test nativo de reservas y el intercambio de tokens cubren partes distintas del contrato, pero no sustituyen ese escenario de sesión completa.

Los pendientes funcionales están enumerados en §12 del plan. No se modificaron ni migraron manualmente los datos de la biblioteca del usuario durante estas comprobaciones.

## Corrección posterior: creación automática de la carpeta de portadas

La captura del usuario mostró un aviso por `%APPDATA%\Ayther\covers` inexistente. Por indicación del usuario, Play ahora crea la carpeta configurada de portadas y sus padres faltantes desde el trabajador, antes de inventariarla. Conserva las portadas existentes. Si no puede crearla, registra el error y mantiene disponible el escaneo del resto de la biblioteca. Si recrea una carpeta previamente indexada en el mismo volumen, actualiza su identidad; la comprobación de volumen sigue impidiendo adoptar silenciosamente otro disco.

Pasaron las tres pruebas específicas que cubren creación de padres faltantes, conservación de portadas, recreación con actualización de identidad y errores cuando un archivo ocupa la ruta. Clippy y la compilación Release también finalizaron correctamente. La revisión anterior también pasó las dos pruebas de inventario y las cuatro de traducciones. La suite completa registrada arriba no se volvió a ejecutar para este cambio. Los mensajes de carpetas no disponibles muestran un resumen breve en la interfaz y registran los detalles técnicos en el log.

La compilación con creación automática se entrega por separado en `artifacts/cache-incremental-covers-create/`. Reemplaza el comportamiento de `cache-incremental-covers-fix`, que omitía la carpeta ausente. No se verificó visualmente la GUI.
