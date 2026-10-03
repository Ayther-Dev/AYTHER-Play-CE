# Plan de caché incremental de catálogo y packs

Fecha: 26 de septiembre de 2026. Base revisada: `cb4f290`.
Estado: implementación en curso. La base funcional ya modifica Play CE; el cierre de todas las fases y la aceptación de §10 siguen pendientes. Ver el estado verificable en §12.
Revisión QA: incorpora decisiones y pruebas para los 24 hallazgos de la revisión del plan. Los límites de rendimiento son objetivos de aceptación, todavía no resultados medidos de Play.

## 1. Decisión

Usar **SQLite para catálogo, metadatos de packs, resultados de validación y estado por juego**. Mantener **TOML para preferencias y rutas configurables**. Conservar las miniaturas como archivos en la caché de imágenes existente.

La mejora principal será eliminar trabajo repetido: identificar ROMs y guardar inspecciones de packs por revisión del archivo; las pantallas consultan los resultados guardados. La primera utilización de un pack externo y la pérdida de continuidad de cambios tienen reglas explícitas en §4–6. Cambiar únicamente JSON por SQLite no elimina los recorridos ni las auditorías actuales.

SQLite encaja con una aplicación de escritorio que necesita consultas parciales y actualizaciones transaccionales. Su documentación incluye expresamente los catálogos multimedia como caso de uso. TOML está orientado a configuración legible. La recomendación de rendimiento para Play se apoya además en la medición siguiente. [SQLite: usos apropiados](https://www.sqlite.org/whentouse.html), [formato de aplicación](https://www.sqlite.org/appfileformat.html), [especificación TOML](https://toml.io/en/v1.1.0).

## 2. Qué hace trabajo innecesario actualmente

| Punto revisado | Comportamiento actual | Sustitución propuesta |
|---|---|---|
| `library::scan_incremental` | Recorre todos los directorios; reutiliza CRC cuando tamaño y fecha coinciden | Inventario por directorio; procesar eventos y archivos cambiados |
| `main::do_scan` | Indexa las carpetas de packs, calcula compatibilidad y reconstruye el catálogo | Aplicar cambios concretos a DB y modelos de UI |
| `library::find_covers` | Busca portadas recorriendo carpetas por juego | Índice de portadas por directorio y referencias persistidas |
| `pack_details::read` | Abre el pack, audita, vuelve a consultar créditos y compatibilidad | Consultar una única instantánea persistida del pack |
| Preparación del lanzamiento | Consulta de nuevo capacidades, metadatos, ficha y créditos | Comprobar revisión del pack seleccionado y reutilizar su instantánea |
| `Library::save_game` / `record_session` | Reescriben el JSON completo | Actualizar las filas afectadas |
| `Library::prune_missing` | Comprueba globalmente si existen los archivos | Bajas limitadas al ámbito reconciliado y accesible |

Referencias de la base revisada: [library.rs](C:/Users/david/Workspaces/ayther/AYTHER-Play-CE/src/library.rs:363), [main.rs](C:/Users/david/Workspaces/ayther/AYTHER-Play-CE/src/main.rs:3340), [pack_details.rs](C:/Users/david/Workspaces/ayther/AYTHER-Play-CE/src/pack_details.rs:156), [pack_install.rs](C:/Users/david/Workspaces/ayther/AYTHER-Play-CE/src/pack_install.rs:165).

## 3. Comparación medida: SQLite, JSON y TOML

Prueba sintética ejecutada en esta máquina con Rust optimizado, 14, 1.000 y 10.000 juegos, metadatos representativos y un pack cada cuatro juegos. Cada resultado es la mediana de nueve operaciones después de calentamiento. La segunda ejecución quedó guardada como resultado reproducible.

Tiempos en milisegundos, con **10.000 juegos**:

| Operación | SQLite | JSON único | TOML único |
|---|---:|---:|---:|
| Leer y materializar todos los juegos | 20,31 | 16,89 | 309,55 |
| Obtener los primeros 60 juegos | 0,086 | 16,95 | 396,46 |
| Obtener un directorio de 100 juegos | 0,142 | 16,90 | 340,91 |
| Cambiar un favorito y guardar | 1,90 | 16,32 | 364,06 |

Con **1.000 juegos**, consultar 60 registros dio 0,073 ms en SQLite, 1,71 ms en JSON y 17,53 ms en TOML. También se probó un TOML separado por directorio: para 100 registros dio 1,31 ms de lectura y 4,46 ms de escritura en ese conjunto. Dividir TOML mejora mucho respecto a un archivo monolítico, pero exige mantener índices y consistencia entre archivos desde la aplicación.

**Interpretación:** SQLite es la opción recomendada para las consultas y modificaciones parciales que necesita esta caché. JSON resultó algo más rápido que SQLite al materializar los 10.000 registros completos; por eso el diseño debe consultar lo visible y evitar cargar toda la biblioteca. Con la biblioteca pequeña actual, ahorrar lecturas y auditorías de packs es prioritario frente al formato del catálogo.

Límites de la prueba:

- Cachés del sistema operativo calientes y conexión SQLite reutilizada. No mide arranque en frío, interfaz, ROMs ni auditoría de archivos `.ay`.
- SQLite 3.46.0, disponible localmente, con WAL y `synchronous=FULL`; TOML 0.8.23 y Serde JSON. La versión de SQLite del ensayo **no es la propuesta para producción**.
- SQLite contiene columnas indexadas y metadatos JSON por registro, que se deserializan al leer. El esquema final podrá normalizar más campos.
- JSON/TOML leen y parsean el documento para seleccionar registros. La escritura parte del catálogo en memoria, serializa, sincroniza el archivo temporal y lo publica. El JSON actual de Play no hace exactamente esa sincronización; no se presentan estos números como tiempos medidos de su implementación actual.
- La sincronización del temporal no demuestra por sí sola durabilidad del renombrado ante pérdida de energía. No se ensayaron fallos de alimentación ni se comparó el espacio total con WAL.
- Hay variación entre ejecuciones, especialmente en escrituras. Es un ensayo exploratorio; repetir con el esquema y SQLite definitivos para evaluar los objetivos de §10. El ensayo no demuestra su cumplimiento ni permite rebajarlos automáticamente.

[Código del benchmark](C:/Users/david/Workspaces/ayther/AYTHER-Play-CE/artifacts/catalog-cache-bench/src/main.rs) y [resultados completos](C:/Users/david/Workspaces/ayther/AYTHER-Play-CE/artifacts/catalog-cache-bench/results.txt). El ensayo está aislado bajo `artifacts`, ignorado por Git, y no añade dependencias a Play.

## 4. Contrato funcional y alcance de las comprobaciones

### 4.1. Vocabulario y contadores

Estas definiciones son normativas para implementación y QA:

| Operación | Alcance | Momento permitido |
|---|---|---|
| Comprobación de atributos | Identidad, tamaño, marcas temporales y token disponible; no lee contenido | Detección de cambios, entrada a ficha y preparación del lanzamiento |
| Lectura de metadatos del pack | Abre `.ay` y lee manifiesto, índice, créditos y descriptores necesarios; no recorre assets para verificar hashes | Descubrimiento inicial o inspección de una revisión pendiente |
| Auditoría de integridad | Comprueba estructura e integridad de las entradas con el validador del core | Instalación; primera utilización de pack externo/migrado; revisión cambiada o no confirmada; reparación explícita |
| Compatibilidad | Evalúa los datos guardados frente a ROM, perfil, core, Runtime y versión de reglas | Al establecer o cambiar ese contexto; sin releer assets |
| Confianza | Evalúa identidad del firmante, registro y política aplicable | Al inspeccionar o cambiar/expirar ese contexto; separada de integridad |
| Identificación de ROM | Calcula CRC32 y SHA-256 en la misma lectura; resuelve metadatos del DAT | ROM nueva, cambiada, seleccionada con revisión no confirmada o reparación explícita |
| Controles de Runtime | Apertura de recursos y controles de firma/integridad propios del motor | Durante su lanzamiento y ejecución; se conservan |

«Validar una vez» significa **una auditoría completa exitosa de Play por revisión confirmada de pack y versión de reglas de integridad**. No incluye nuevas reglas de integridad, reintentos tras cancelación/error, revisiones modificadas, revisión no confirmada ni las comprobaciones independientes de Runtime. Una regla de compatibilidad nueva no invalida por sí sola la auditoría de bytes.

Contar separadamente `metadata_checks`, `pack_metadata_reads`, `pack_content_opens`, `pack_audit_attempts`, `pack_audit_successes`, `pack_audit_bytes`, `rom_hash_bytes`, `directories_enumerated`, `db_rows_changed` y tiempos de UI/DB. Abrir un handle solo para atributos se cuenta como `metadata_checks`, no como `pack_content_opens`. Toda apertura para leer manifiesto/ZIP/assets sí cuenta como contenido. Identificar el proceso: la exigencia de cero aperturas corresponde a Play; los controles de Runtime se miden aparte y nunca se omiten para cumplirla.

### 4.2. Disparadores y secuencia de uso

Abrir el catálogo, buscar, desplazarse o resaltar una tarjeta **no dispara auditorías**. Abrir la ficha comprueba la ROM y el pack guardado como seleccionado. Si falta una revisión confirmada, comienza su identificación/auditoría en segundo plano. Cambiar de pack dentro de esa ficha aplica la misma regla al nuevo seleccionado; abrir un selector o pasar el foco por opciones no equivale a seleccionar.

Pulsar Jugar, también desde accesos directos, ejecuta el mismo control de preparación. Se une al trabajo ya existente para ese archivo/revisión; no inicia una segunda auditoría. El lanzamiento espera la revisión confirmada de ROM, la auditoría aceptable del pack y un resultado de compatibilidad/confianza admisible para el contexto elegido. Si identificar la ROM revela otro contenido, cancelar esa intención de lanzamiento, actualizar la ficha y pedir una nueva acción Jugar sobre el juego identificado; no lanzar otro juego por conservar la ruta. Jugar sin pack omite toda comprobación de packs, pero conserva las de ROM y core. Cancelar una preparación cancela la intención de lanzamiento: terminar un trabajo compartido no debe iniciar el juego después.

| Acción | Trabajo permitido |
|---|---|
| Arranque con caché confirmada y sin cambios | Inicio desde DB, atributos/eventos; cero contenido de ROMs/packs |
| Detectar ROM nueva/cambiada | Identificar únicamente esa ROM; actualizar sus datos y asociaciones |
| Recibir cambio localizado | Actualizar el archivo o enumerar su directorio; recorrer subárboles nuevos/desconocidos |
| Instalar pack | Copiar, extraer metadatos y auditar staging; persistir la revisión promovida |
| Descubrir pack externo nuevo | Leer sus metadatos una vez; auditoría pendiente hasta su utilización |
| Cambiar pack no seleccionado | Marcar datos anteriores como desactualizados; posponer inspección hasta utilizarlo |
| Abrir ficha/preparar juego con revisión vigente | Atributos y DB; cero aperturas de contenido `.ay` por Play |
| Abrir ficha/preparar juego con revisión pendiente | Una tarea compartida de inspección del pack seleccionado, con progreso |
| Desinstalar/restaurar | Operación sobre ese pack y todos sus vínculos; cero escaneo global |
| Favorito/tiempo jugado | Transacción sobre el estado del juego; cero exploración de archivos |

### 4.3. Estados y acciones de la interfaz

El estado de disponibilidad, la vigencia del contenido y el resultado de compatibilidad son dimensiones separadas. Tener metadatos guardados **no significa** tener auditoría vigente.

| Estado mostrado | Acción con pack | Otras acciones |
|---|---|---|
| Metadatos disponibles; auditoría pendiente/no confirmada | Preparar una comprobación; no lanzar todavía | Ver ficha, cambiar pack, jugar original si ROM/core están listos, desinstalar |
| Comprobando | Esperar el trabajo compartido con progreso; no lanzar todavía | Cancelar la intención de uso, cambiar selección o jugar original |
| Vigente y compatible | Permitir lanzamiento después del control final de revisión | Acciones normales |
| Vigente e incompatible | Bloquear lanzamiento con ese pack y explicar el motivo | Desinstalar, seleccionar otro o jugar original |
| Integridad inválida/error de inspección | Bloquear lanzamiento; mostrar causa y reintento explícito | Desinstalar, seleccionar otro o jugar original |
| Archivo no disponible/en modificación | No lanzar ni modificar ese archivo | Conservar ficha anterior marcada como no vigente; seleccionar otra opción |

`MissingIndex` se muestra como «Sin índice de integridad» y conserva la decisión de admisibilidad del core; jamás se etiqueta «Integridad verificada». Un contexto incompleto muestra «Compatibilidad pendiente» y enumera ROM/core/Runtime/perfil faltantes. Ningún estado global bloquea la navegación por el resto del catálogo.

## 5. Detección, continuidad e identidad

### 5.1. Contrato de «solo lo que cambió»

Hay dos regímenes explícitos. **Incremental con continuidad:** solo se procesan archivos/directorios señalados por eventos fiables. **Reconciliación por continuidad perdida:** se enumeran atributos del menor subárbol cuya continuidad no puede demostrarse; puede abarcar una raíz completa. Este segundo régimen es una excepción funcional al requisito original de recorrer exclusivamente directorios ya identificados como cambiados. No se declara cumplimiento de ese requisito estricto en dicho régimen.

El régimen se registra por raíz junto con el motivo y alcance. Sin diario utilizable al reiniciar, la raíz entra en reconciliación; no se presupone que estuvo sin cambios. Inicio se muestra desde caché mientras esto ocurre. La búsqueda de diferencias lee atributos; **los packs nuevos descubiertos sí requieren una lectura inicial de metadatos**, separada y contada. Los packs existentes cambiados/no confirmados quedan pendientes; no hay auditoría masiva. Una primera biblioteca sin caché requiere inventario inicial completo con progreso.

La fecha de un directorio no prueba la vigencia de sus descendientes. Evaluar la disponibilidad real del diario USN de NTFS sin elevar Play: identificador de diario y posición por volumen. Si no puede consultarse con los permisos del usuario, usar reconciliación. El plan no presupone que USN será viable en todos los equipos. [Microsoft: identificador del diario](https://learn.microsoft.com/en-us/windows/win32/fileio/using-the-change-journal-identifier).

### 5.2. Revisión de archivo y cambios invisibles a los atributos

Una revisión contiene la identidad de volumen/archivo cuando existe, ruta resuelta, tamaño, fecha de escritura con precisión disponible, fecha de cambio cuando existe y token nativo disponible. **Cualquier diferencia o evento de escritura/reemplazo invalida**, aunque después se restauren tamaño/fecha. Un renombrado puro demostrado por identidad y continuidad actualiza ubicación sin volver a leer contenido. Una diferencia de atributos puede producir un falso positivo: es preferible a publicar datos de otra revisión.

Una revisión se considera confirmada si coinciden todos los campos que soporta ese sistema y hay continuidad de observación desde su lectura exitosa. Un campo opcional no soportado se registra como tal; un campo antes disponible que ya no puede consultarse invalida la confirmación. Si fallan atributos obligatorios (ruta accesible, tamaño o fecha), el archivo es no disponible. Sin identidad nativa, la ruta y una generación de descubrimiento identifican la entrada; no se deducen movimientos por nombre.

Tras una brecha de eventos, el simple empate de atributos produce **revisión no confirmada**, no confirmada. Se mantienen datos visibles como anteriores; al abrir la ficha o preparar el juego se identifica solo esa ROM y se audita solo el pack seleccionado. Así, un cambio de contenido con tamaño/fecha conservados durante el cierre no reutiliza silenciosamente su CRC o auditoría al usarlo. Si la nueva lectura produce el mismo digest y no hubo eventos durante ella, vuelve a confirmarse sin cambiar identidad lógica. El resto de archivos permanece pendiente hasta utilizarse.

La vigilancia debe poder observar escrituras para mantener confirmación. Si el proveedor de archivos no garantiza notificaciones (por ejemplo una ubicación no compatible), la confirmación dura solo durante la preparación/uso protegido; cada uso posterior se vuelve a comprobar. Nunca se promete detectar escrituras que el sistema no notifica ni refleja en sus atributos sin releer contenido. Mutaciones que evadan incluso los controles del sistema operativo quedan fuera de esta garantía de caché; no se presenta la caché como una prueba criptográfica actual.

### 5.3. Eventos, saturación y puntos de recuperación

Observar las raíces antes de inventariarlas con `ReadDirectoryChangesW` o un adaptador equivalente. Agrupar eventos durante 250 ms, con espera máxima de 1 s desde el primero para evitar postergación indefinida. Procesar el archivo nombrado; enumerar su padre si el evento no basta. Enumerar recursivamente solo subárboles nuevos o declarados desconocidos. La deduplicación de eventos propios compara revisión y generación, nunca ignora una ventana temporal completa.

Límites iniciales: 4.096 eventos en memoria y 256 trabajos únicos pendientes; máximo dos lectores pesados, uno por volumen. Si un límite se excede, marcar el ámbito del observador como `dirty` y compactar eventos en una tarea de reconciliación; no descartarlos manteniendo el ámbito como limpio. Un trabajador fallido deja su trabajo pendiente con causa. Priorizar la preparación del juego sobre mantenimiento, sin multiplicar lectores.

Persistir resultados y posición consumida del diario en la misma transacción; avanzar solo hasta el tramo contiguo completamente aplicado. Un evento de pack puede considerarse aplicado después de persistir su invalidación, aunque la auditoría siga pendiente. Guardar al inicio que la sesión está abierta; el cierre es limpio solo después de confirmar los trabajos/eventos pendientes o dejar explícitamente su ámbito `dirty`. Caída, cierre forzado o fallo al persistir invalidan cualquier suposición basada exclusivamente en la vigilancia de esa sesión. Sin diario recuperable, reconciliar el ámbito afectado al próximo arranque.

El desbordamiento del observador, saturación interna, reinicio de diario y pérdida de continuidad siguen la misma regla. Las generaciones impiden que un resultado anterior limpie un ámbito con eventos más recientes. [Microsoft: pérdida de notificaciones](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw).

### 5.4. Disponibilidad, movimientos y archivos en escritura

Una raíz inaccesible por desconexión, permisos, error de I/O o ruta ausente permanece no disponible, con motivo; no se borra automáticamente. Una baja de archivo solo se confirma cuando su directorio padre se enumeró completamente y sin errores, en el volumen esperado, y el archivo falta. Quitar una raíz desde Ajustes la desregistra y oculta solo entradas que ya no pertenezcan a ninguna otra raíz activa, sin borrar archivos ni estado histórico. Una nueva raíz en la misma ruta no hereda datos por ese solo hecho.

Identificar volúmenes por identidad, no por letra. Una letra reutilizada por otro disco no satisface la disponibilidad del anterior. Un volumen conocido en una nueva ubicación conserva vínculos solo al resolver su identidad. IDs de archivo reutilizados después de una baja o brecha no demuestran identidad de contenido.

Reglas de juegos: un renombrado demostrado dentro del volumen conserva el ID. Una ROM cuyo SHA-256 cambia representa otro contenido: crear otro juego y conservar el estado anterior como histórico, aunque coincidan ruta o CRC32. Una copia simultánea es otra entrada y no recibe automáticamente favoritos/estadísticas. Un movimiento entre volúmenes conserva estado solo después de identificar la ROM nueva y encontrar exactamente un origen confirmado como ausente con el mismo SHA-256; si hay varios candidatos, se mantienen entradas independientes. La importación de JSON conserva sus asociaciones existentes, pero no inventa identidad fuerte que antes no se guardaba.

Deduplicar raíces superpuestas y aliases que resuelvan a la misma entrada nativa. Respetar la semántica de mayúsculas del directorio; no convertir todas las rutas a minúsculas como identidad. Detectar ciclos de enlaces; una ruta que sale del conjunto de raíces autorizado no amplía silenciosamente el inventario.

Para archivos externos, esperar 1 s sin eventos antes de inspeccionar y adquirir un acceso de lectura que excluya escritores durante la lectura cuando el sistema lo permita. Esto no demuestra que una copia pausada haya terminado: el resultado se vincula a esa revisión, y una escritura posterior lo invalida. Las extensiones de staging conocidas no se catalogan. Si no puede obtenerse acceso estable, mostrar «Archivo en modificación» y no publicar un resultado confirmado. Un error mientras cambia se marca pendiente; un error sobre revisión estable se muestra como inválido, y se reintenta automáticamente cuando llegue otro cambio, o por acción explícita. Nunca transferir estado histórico a una ROM parcial por coincidencia de ruta.

### 5.5. Dependencias externas

| Cambio | Invalidación y trabajo autorizado |
|---|---|
| DAT | Versionar por contenido; resolver otra vez metadatos usando los CRC guardados; no hashear ROMs intactas; conservar personalizaciones |
| Portada creada/modificada/borrada | Actualizar índice del directorio, vínculos afectados y miniaturas de esa revisión; usar imagen alternativa si falta |
| Rutas de ROMs/packs/portadas | Registrar o desregistrar únicamente ámbitos afectados con las reglas de disponibilidad anteriores |
| Core/Runtime añadido, reemplazado o eliminado | Invalidar compatibilidad de contextos afectados; resolver con metadatos cacheados al mostrar/usar, sin auditar packs |
| Registro/política de confianza | Invalidar decisiones de confianza afectadas; no volver a auditar assets por ese único cambio |
| Regla de integridad nueva | Invalidar auditoría de revisiones afectadas; reauditar al seleccionarlas, nunca todas al arrancar |

El contexto usa identidad/revisión del ejecutable o core además de su nombre de versión; reemplazar bytes conservando la versión no conserva el resultado. Si la política tiene vencimientos, la decisión caduca en ese instante; un retroceso de reloj invalida su vigencia hasta reevaluarla. Estos trabajos tienen sus propios eventos/versiones y no simulan cambios en ROMs/packs.

## 6. Packs, transacciones de archivos y partidas

### 6.1. Datos y contexto de compatibilidad

`PackRepository` entrega una instantánea con metadatos, revisión y resultados separados de integridad, confianza y compatibilidad. Su clave de compatibilidad incluye revisión de pack, identidad/CRC de ROM confirmada, perfil elegido, revisión de core/Runtime, opciones relevantes y versión del validador. No existe un único «compatible» global para todos los juegos/perfiles.

Instalar sin ROM/core/Runtime es válido si pasa la admisibilidad de instalación del core: guardar auditoría y datos, dejando contextos incompletos como pendientes. Evaluar con los datos guardados los contextos completos asociados; otros perfiles se evalúan al elegirlos. Reutilizar la implementación del core, ampliando su API para consumir datos persistidos si hoy exige una ruta. El esquema debe conservar las evidencias necesarias para esas reglas; una regla que requiera evidencia no guardada deja el contexto pendiente y solicita inspección específica al usarlo, sin fingir un resultado.

Un pack externo nuevo permite una instantánea **solo de metadatos**, con auditoría pendiente. La garantía de cero aperturas de contenido comienza después de la primera inspección aceptable, con revisión confirmada y contexto vigente. Packs migrados sin prueba persistida siguen el mismo proceso.

### 6.2. Asociación de múltiples packs

La identidad interna de instalación/archivo es distinta del identificador declarado por el pack. Dos archivos con igual identificador declarado se muestran como opciones distintas, con versión y ubicación; no se fusionan ni reemplazan automáticamente. Un destino de instalación ocupado requiere la acción explícita de reemplazo de esa instalación.

Actualizar o eliminar un pack actualiza en una transacción **todos** sus juegos asociados. Si una revisión ya no admite una ROM, retirar ese vínculo y dejar visible la instalación en el gestor de packs. Conservar como referencia histórica la selección anterior, mostrar «Pack seleccionado no disponible/incompatible» y ofrecer originales u otra selección; no activar otro pack ni lanzar sin pack silenciosamente. Una selección histórica no mantiene activo un vínculo inválido. Si cambia el manifiesto de un pack no seleccionado, sus asociaciones antiguas se muestran pendientes hasta inspeccionarlo y no autorizan lanzamiento.

### 6.3. Exclusión, promoción y cancelación

Serializar mutaciones por instalación/archivo con bloqueo que también cubra otros procesos de Play. Dos solicitudes de auditoría de la misma revisión comparten un trabajo y resultado; cada consumidor puede retirar su interés. Cancelar un consumidor no cancela a los demás; sin consumidores, solicitar cancelación del trabajo. Una nueva revisión invalida el resultado anterior aunque la tarea termine después.

Fases persistidas de instalación/restauración: `preparing`, `inspected`, `promoting`, `committed`, `cleanup`. Copia e inspección pueden cancelarse; conservar la versión instalada anterior y eliminar el staging propio cuando deje de estar en uso. Desde `promoting`, la operación no es cancelable: mostrar «Finalizando…» y completar o recuperar la transacción. Desinstalación registra primero su intención y una retirada recuperable; la eliminación física final ocurre después de confirmar DB. No borrar archivos externos basándose únicamente en una ruta almacenada: verificar la identidad objetivo.

Inspeccionar staging con acceso estable, comparar revisión antes/después y comprobar que el archivo promovido es el inspeccionado. Persistir digest, revisión final y resultados antes de anunciar éxito. Si se rompe la exclusión, cambia la identidad o falla la comprobación, no transferir la auditoría al destino. El registro de recuperación identifica instalación, archivos y fases, no solo nombres reutilizables. Si filesystem y DB discrepan tras un fallo, el pack permanece pendiente de recuperación y sin lanzamiento; conservar archivos para resolver la operación. No se promete atomicidad conjunta de SQL y renombrados.

Una auditoría que falla o se cancela no cuenta como «una vez exitosa». Los reintentos solo responden a un cambio nuevo, solicitud explícita o recuperación identificada; no existe un bucle automático de auditorías sobre un archivo inválido estable.

### 6.4. Ventana entre auditoría y lanzamiento; pack en uso

La preparación toma una reserva de lectura de la ROM y del pack seleccionados y comprueba sus revisiones frente a los resultados persistidos. La reserva de Play no abre lectores ZIP ni lee assets: cuenta como control de atributos. Antes de aceptar una confirmación cacheada, procesar los cambios anteriores a la reserva hasta una barrera de observación verificable; si el proveedor no permite establecerla ni consultar un token actual fiable, tratar la revisión como no confirmada y leerla bajo reserva. La auditoría necesaria puede abrir contenido y se cuenta como tal. Mantener las reservas hasta que Runtime las asuma sobre los mismos archivos y confirme identidad; si la revisión cambia antes, invalidar preparación y volver a §4.2. La UI distingue «Preparando», «El archivo cambió» y «En ejecución»; jamás presenta como lanzada una revisión distinta de la preparada.

**Dependencia de implementación:** coordinar este contrato de reserva/confirmación con Runtime. En Windows la reserva debe impedir nuevas escrituras/reemplazos/eliminaciones mientras se usa el archivo; no basta con un mutex en Play ni con comparar la ruta. Si no puede obtenerse acceso estable, no lanzar ese archivo y explicar el motivo; si afecta solo al pack, el juego original continúa siendo una opción. Los controles propios de Runtime siguen aplicándose al contenido realmente abierto.

Runtime mantiene la reserva durante la partida, incluso si Play se cierra. Deshabilitar instalación de reemplazo, restauración y desinstalación de esa instalación mientras exista una sesión en uso; responder «Pack en uso» sin modificar archivos ni encolar una mutación silenciosa. La selección para una partida futura puede cambiar; la sesión actual conserva su revisión. Registrar por separado revisión en ejecución y revisión observada en disco. Si un proveedor externo rompe la reserva y cambia el archivo, no adoptar esa revisión ni recargarla automáticamente: notificar el fallo a Runtime para detener el uso del pack/sesión con diagnóstico claro, conservando los guardados existentes. Este caso debe probarse con el contrato Runtime, no darse por resuelto con la DB.

## 7. Persistencia, recursos y errores

### 7.1. Modelo y ejecución

| Entidad | Información principal |
|---|---|
| `roots`, `directories` | Identidad, jerarquía, disponibilidad, régimen de detección, generación y estado `dirty` |
| `files` | Ruta, identidad física, clase y revisión observada/confirmada |
| `games` | Identidad lógica, archivo, CRC/SHA-256, versión DAT y portada |
| `game_user_state` | Favorito, personalizaciones, selección actual/histórica, última partida y duración |
| `packs`, `pack_revisions` | Instalación, archivo, metadatos, auditoría y decisiones de confianza por revisión |
| `game_packs`, `compat_results` | Asociaciones y resultados por contexto completo |
| `change_checkpoints`, `pending_operations` | Continuidad confirmada, trabajos recuperables y fases de filesystem |
| `active_sessions` | ID de sesión, revisión en uso y estado de reserva/confirmación con Runtime |

CRC32 sirve para compatibilidad, no es clave única. Indexar identidad/ruta, padre, CRC, asociaciones, orden estable del catálogo y últimas partidas. Usar columnas para campos consultados; JSON interno para listas pequeñas no filtradas. Cambios de identidad y archivo no borran en cascada el estado histórico del usuario. Registrar duración por ID de sesión para que una recuperación no la sume dos veces.

DB local por usuario: `%LOCALAPPDATA%\Ayther\catalog.sqlite3`; preferencias TOML en su ubicación actual. El modo portable admite una DB en almacenamiento local/extraíble accesible; no admitir DB en una ruta de red. Un cambio de ubicación de DB es una operación explícita con Play/Runtime sin mutaciones activas y copia consistente; no reinterpretar `lib_path` como DB sin migración.

`rusqlite` con SQLite incluido, un hilo propietario de DB y sentencias preparadas. Las lecturas pesadas/inspecciones ocurren fuera de transacciones. Lotes máximos de 100 filas y objetivo de 20 ms por transacción en el equipo de referencia; replanificar entre lotes. Ningún acceso de disco, espera de bloqueo o canal bloqueante en Slint. Respetar límites de colas/concurrencia de §5.3.

Configuración: `foreign_keys=ON`, WAL, `synchronous=FULL`, `busy_timeout=2000` ms. Checkpoints fuera de UI. Usar SQLite 3.51.3 o posterior con la corrección WAL y compatible con el toolchain; fijar dependencia. La versión antigua usada en el benchmark no se incorpora a Play. [Durabilidad](https://sqlite.org/pragma.html#pragma_synchronous), [rusqlite](https://docs.rs/crate/rusqlite/latest), [corrección WAL](https://sqlite.org/wal.html#walresetbug).

### 7.2. Fallos e instancias concurrentes

Una única instancia escritora por DB: una segunda apertura entrega la activación a la primera y termina; si no logra comunicarse pero existe el bloqueo, informa que la biblioteca está en uso y no escribe. Distintas DB que apuntan a la misma carpeta de packs respetan los bloqueos por archivo/instalación y las reservas de Runtime; un conflicto devuelve «En uso» sin cambios.

| Fallo | Resultado requerido |
|---|---|
| Disco lleno, DB de solo lectura o fallo de escritura | No anunciar favorito, instalación o estadísticas como guardados; revertir SQL, conservar última vista persistida y mostrar error/reintento |
| Bloqueo mayor de 2 s | Terminar esa solicitud con error recuperable; UI sigue operativa; no reintentos infinitos |
| DB corrupta | No recrearla vacía ni sobrescribirla; conservar original, mostrar diagnóstico y ofrecer recuperar una copia válida |
| Fallo de lectura de ROM/pack | Marcar solo ese archivo no disponible/no confirmado; no convertir error de I/O en baja o integridad inválida |
| Pérdida de DB durante una mutación de pack | Conservar operación/archivos recuperables; bloquear uso de la instalación afectada hasta resolverla |

En modo de solo lectura se puede navegar la última instantánea legible y lanzar archivos cuya preparación resulte válida con resultados temporales; indicar antes de iniciar que las estadísticas no se guardarán. No habilitar instalación/desinstalación/restauración ni cambios de preferencias persistentes como si fueran exitosos. Si no puede leerse la DB, no se presume que una caché parcial sea completa.

### 7.3. Retención y copias

Copias consistentes mediante API de backup antes de migraciones y después de un cierre limpio con cambios; conservar tres copias válidas. Verificar una copia antes de sustituir otra. No copiar únicamente el archivo principal con WAL activo. [SQLite Backup API](https://www.sqlite.org/backup.html).

Limpiar datos derivados no elimina juegos históricos, preferencias ni estadísticas. El recolector puede retirar resultados de revisiones sin referencias después de 30 días, pero nunca archivos de packs/ROMs, operaciones pendientes, revisiones activas ni estado del usuario. Miniaturas en disco: LRU de 1 GiB; memoria decodificada: 128 MiB. Si WAL supera 64 MiB, solicitar checkpoint; a 256 MiB pausar ingestión de fondo y diagnosticar lector/bloqueo, sin borrar WAL ni interrumpir la UI. Reducir WAL por debajo de 64 MiB tras liberar lectores y completar checkpoint. Las copias y el estado histórico tienen tamaño visible; no se sacrifican para cumplir un límite de caché.

## 8. UI, cancelación y migraciones

### 8.1. Inicio y navegación consistente

Presentar ventana maximizada en Inicio. El sondeo del core no desencadena escaneo global. Mostrar hasta 60 registros iniciales y cargar portadas visibles/próximas, con páginas por orden estable `(clave de orden, game_id)` y generación de catálogo. No mantener una transacción SQL abierta mientras el usuario navega.

Al llegar cambios, aplicar la nueva generación entre interacciones conservando selección por ID y ancla de desplazamiento; invalidar páginas antiguas y descartar respuestas tardías. No mezclar páginas de distintas generaciones. Si cambia el orden de la selección, mantener su foco sin devolver al principio; si se elimina, seleccionar el siguiente ID visible o el anterior si era el último. Catálogo vacío: foco en la acción principal disponible. Cambio de filtro conserva selección solo si sigue incluida; no altera la selección de pack guardada.

### 8.2. Progreso y cancelación

Mostrar etapas: «Buscando juegos nuevos…» sin porcentaje hasta conocer total; «Identificando juegos: n de N»; «Copiando pack…» y «Comprobando pack…» por bytes/entradas; «Comprobando cambios…» para reconciliación. Emitir como máximo diez actualizaciones por segundo, y siempre un estado terminal. Si cambia el total, explicitarlo sin simular un porcentaje global monótono. Un acierto de caché no muestra barra.

Responder a Cancelar en UI en ≤100 ms y mostrar «Cancelando…» hasta que el trabajador libere recursos. Revisar cancelación entre bloques de lectura de como máximo 1 MiB/entradas; las APIs bloqueadas deben solicitar cancelación de I/O. Si el sistema no responde, mostrar espera del dispositivo, nunca «Cancelado» antes de terminar. Resultados tardíos de la generación cancelada no se publican. El paso no cancelable de promoción está definido en §6.3.

Al cancelar un inventario conservar los juegos ya confirmados por lotes y marcar el ámbito incompleto; no ejecutar bajas sobre un inventario parcial. Primera ejecución: mostrar lo descubierto y «Exploración incompleta», o estado vacío con esa indicación si aún no hay juegos; permitir reanudar. Cancelar una auditoría conserva metadatos anteriores como no vigentes y no habilita lanzamiento con el pack. La cancelación de un consumidor respeta los trabajos compartidos de §6.3.

### 8.3. Migración, versiones y recuperación

Importar JSON una vez a una DB nueva, en transacción, conservando el archivo original. Comprobar conteos, IDs y valores de favoritos, nombres/portadas, selección y estadísticas antes de activar la nueva DB. Guardar una marca de importación para no duplicar datos al reintentar. Auditorías no documentadas quedan pendientes; hashes/identidades ausentes se completan al utilizar el archivo. JSON inválido, activación fallida o importación interrumpida dejan la fuente intacta y muestran un diagnóstico.

Migraciones de esquema bajo exclusión de instancias, copia previa y transacción; avanzar versión solo con éxito. Si se interrumpen, el siguiente arranque observa el esquema anterior o el nuevo completo. Una operación excepcional no transaccional requiere fase recuperable explícita antes de introducirse. Una versión anterior de Play que encuentre esquema más nuevo rechaza abrirlo y pide la versión compatible; no hace downgrade, no recrea DB y no reutiliza un JSON obsoleto como si fuera actual.

Al restaurar una copia, conservar la DB sustituida, invalidar checkpoints y confirmaciones de archivos y reconciliar metadatos. Inspeccionar operaciones pendientes y reservas reales de Runtime antes de mutar archivos: una marca antigua de la copia no prueba que sigan iguales ni que una partida siga activa. El estado de usuario recuperado corresponde a la fecha de esa copia; informar que cambios posteriores pueden haberse perdido. No prometer recuperación de datos que no existen en ninguna copia legible. Una reconstrucción ordinaria de datos derivados sí conserva íntegro el estado de usuario actual.

## 9. Secuencia y dependencias de implementación

1. **Instrumentación y prueba de continuidad:** contadores de §4.1, régimen por raíz y USN con usuario normal; capturar línea base en el equipo de QA.
2. **DB y recuperación:** esquema, migración, exclusión de instancias, consultas paginadas, backup y errores de almacenamiento; primer Inicio desde DB.
3. **Packs y preparación:** inspección compartida, estados, contexto, progreso y cancelación; registro de mutaciones y contrato de reserva/confirmación con Runtime. La protección de packs en uso es requisito de esta fase, no trabajo implícito fuera del proyecto.
4. **Inventario incremental:** eventos, continuidad, saturación, disponibilidad, identidades y dependencias externas; eliminar escaneo global del arranque confirmado.
5. **UI y aceptación:** generaciones de páginas, foco, límites de recursos y matriz de fallos; medir frío/caliente y ejecutar toda §10.

Preservar múltiples packs, eliminación de incompatibles y arranque maximizado en cada fase. No mantener escritura completa permanente de JSON en paralelo. Si la prueba USN no es viable, implementar y reportar el régimen de reconciliación; no declarar que se logró cero enumeraciones entre arranques para esas raíces.

## 10. Criterios de aceptación y presupuestos

### 10.1. Método y latencia

Los siguientes son **objetivos normativos propuestos**, no resultados obtenidos. Referencia de QA: Windows x64 soportado, SSD local, al menos cuatro núcleos y 16 GiB de RAM, compilación release, sin depurador. Registrar CPU, SO, unidad, versión de Play/SQLite, corpus y carga concurrente para reproducibilidad. Corpus: 14, 1.000 y 10.000 juegos; para 10.000, 1.000 packs registrados, una muestra con múltiples packs por juego y al menos un pack de 200 MiB. Medir por separado escenarios con inspección inicial, cambios y dispositivos lentos.

Ejecutar 30 arranques calientes y diez fríos (primer lanzamiento después de reiniciar), con caché persistida confirmada y corpus intacto. Medir desde creación del proceso hasta el evento observado, sin excluir inicialización de bibliotecas. No usar el benchmark sintético como sustituto de esta prueba.

| Métrica | Aprobación en referencia local |
|---|---|
| Primer frame visible, ventana maximizada en Inicio | p95 ≤1 s caliente y ≤2 s frío |
| Inicio utilizable: primera página, foco, navegación y botón Jugar responden | p95 ≤1,5 s caliente y ≤3 s frío; una validación necesaria puede continuar con progreso |
| Consultar/materializar página de 60 registros en DB | p95 ≤50 ms |
| Ficha de revisión confirmada, desde entrada hasta datos interactivos | p95 ≤150 ms, sin contar un lanzamiento de Runtime |
| Guardar favorito/estado pequeño, desde solicitud hasta commit | p95 ≤100 ms |
| Latencia de evento de UI durante escaneo/auditoría/checkpoint | p95 ≤50 ms; ninguna pausa atribuible a estas tareas >100 ms |
| Confirmar o invalidar un cambio localizado, archivo ya estable | p95 ≤2 s desde recepción del evento; la auditoría puede seguir pendiente |

No imponer duración absoluta a la copia/auditoría de cualquier tamaño o dispositivo; registrar bytes/s y duración por etapa, exigir progreso y cancelación. En medios extraíbles/red se mantienen límites de respuesta de UI y seguridad de estado; latencias de I/O se reportan aparte. Un incumplimiento requiere investigación y nueva revisión explícita del presupuesto; no se ajusta el umbral a posteriori para aprobar.

Recursos para corpus de 10.000/1.000: memoria privada de Play ≤512 MiB durante navegación y ≤768 MiB durante una inspección de pack de 200 MiB; caché decodificada dentro de 128 MiB. Tras diez ciclos idénticos de navegación/actualización, memoria estable sin crecimiento acumulativo >10% respecto al primer ciclo estabilizado. DB inicial, sin histórico de sesiones, ≤128 MiB; tras 100 reconciliaciones sin cambios, cero nuevas revisiones y crecimiento de DB ≤5%. Aplicar límites de colas, miniaturas y WAL de §5.3/§7.3. Datos históricos legítimos se contabilizan aparte, nunca se borran para pasar la prueba.

### 10.2. Matriz funcional y de fallos

| ID | Escenario | Resultado exigido |
|---|---|---|
| T01 | Arranque confirmado, sin cambios | 0 contenido de ROMs/packs, 0 auditorías, 0 recorridos completos; solo consultas/atributos/eventos |
| T02 | Reinicio sin diario o pérdida de continuidad | Inicio cacheado; régimen y ámbito registrados; atributos reconciliados; 0 auditorías masivas; confirmar solo archivos utilizados |
| T03 | Pack nuevo añadido durante el cierre | Una lectura inicial de metadatos por revisión; auditoría pendiente hasta ficha/uso; no atribuir esa lectura a «cero contenido» |
| T04 | ROM nueva en B | Hash únicamente de nueva ROM; 0 recorridos de raíces no afectadas |
| T05 | Ficha repetida y lanzamiento con pack confirmado/contexto vigente | 0 `pack_content_opens` de Play; controles de atributos y de Runtime separados |
| T06 | Pack con metadatos pero nunca auditado, o modificado | Primera ficha/uso crea una tarea compartida; siguiente acceso a esa revisión no repite auditoría exitosa |
| T07 | Cambio no seleccionado | Invalidación persistida; 0 auditorías hasta utilizarlo; asociaciones anteriores marcadas pendientes |
| T08 | Datos incompletos, pack inválido/incompatible o `MissingIndex` | Estados y botones de §4.3; instalación sin contexto no se etiqueta compatible ni verificada sin prueba |
| T09 | Contenido alterado conservando tamaño/fecha durante brecha | Revisión no confirmada; relectura solo al utilizar; CRC/digest/resultados anteriores no autorizan lanzamiento |
| T10 | Copia pausada y reanudada; archivo bloqueado | Estado ligado a revisión; nueva escritura invalida; acceso inestable no produce confirmación; error de I/O no es baja |
| T11 | ROM reemplazada, copia, movimiento entre volúmenes, CRC duplicado | Identidad/estado según §5.4; sin transferencia por ruta o CRC aislados |
| T12 | Eventos desbordados, cola llena, trabajador caído y cierre forzado | Ámbito `dirty`, checkpoint no adelantado sobre trabajo sin persistir, recuperación sin omitir eventos observables |
| T13 | Cambio durante auditoría, promoción o entre preparación y lanzamiento | Ningún resultado para otra revisión; recuperación localizada o preparación reiniciada; Runtime confirma la reservada |
| T14 | Dos solicitudes, dos instancias y DB distintas con mismo pack | Auditoría compartida por revisión dentro de la instancia; exclusión de mutaciones entre procesos; conflicto visible sin cambios |
| T15 | Cancelación en copia, auditoría, inventario y promoción | Reglas de §6.3/§8.2; sin lanzamiento tardío, bajas por inventario parcial ni éxito ficticio |
| T16 | DAT, portada, core, Runtime, confianza o reglas cambian | Invalidación exacta de §5.5; sin hash de ROM/auditoría masiva por cambios de metadatos o compatibilidad |
| T17 | Pack compartido eliminado/cambia ROMs; IDs declarados duplicados | Actualizar todos los vínculos, conservar historial, no seleccionar automáticamente otro pack |
| T18 | Volumen desconectado, permisos denegados, letra reutilizada o raíz quitada | Sin bajas por error; identidades y visibilidad de §5.4; estado histórico conservado |
| T19 | Disco lleno, read-only, bloqueo >2 s y corrupción | Error preciso, sin éxito falso ni DB vacía automática; conservación/recuperación de §7.2 |
| T20 | Migración interrumpida, esquema más nuevo o copia restaurada | Sin importaciones duplicadas/downgrade; checkpoints invalidados al restaurar; pérdidas posteriores a copia informadas |
| T21 | Altas/bajas/reordenamiento durante paginación | Sin páginas mezcladas, duplicados u omisiones de generación; foco y ancla según §8.1 |
| T22 | Reemplazar/desinstalar/restaurar durante partida; Play se cierra | Reserva de Runtime vigente, mutaciones bloqueadas; sesión no adopta nueva revisión; diagnóstico si el proveedor rompe reserva |
| T23 | Favoritos/duración, incluida recuperación de sesión | SQL acotado, 0 exploraciones/reescrituras globales; duración no duplicada |
| T24 | Limpieza, checkpoints y ciclos repetidos | Estado de usuario íntegro, presupuestos de §10.1 y retención de §7.3; sin borrar archivos de juego/pack |

## 11. Trazabilidad de los hallazgos QA

La numeración siguiente corresponde a los 24 hallazgos recibidos. «Cubierto» significa que el plan define la decisión y una prueba; no que la implementación ya la cumpla.

| Hallazgo | Decisión en el plan | Verificación |
|---|---|---|
| 1. Alcance de comprobación | §4.1, §6.1 | T01, T05, T08 |
| 2. Definición de cambio | §5.2 | T09, T13 |
| 3. Momento de validación | §4.2 | T05, T06 |
| 4. Información pendiente | §4.3 | T07, T08 |
| 5. Identidad de juegos | §5.4 | T11 |
| 6. Instalación sin contexto | §6.1 | T08 |
| 7. Cancelación | §6.3, §8.2 | T15 |
| 8. Disponibilidad/baja | §5.4 | T18 |
| 9. Rendimiento verificable | §10.1 | Presupuestos y T24 |
| 10. Directorios exclusivos frente a reconciliación | §5.1: excepción explícita y régimen observable | T01, T02 |
| 11. Metadatos de packs nuevos al reconciliar | §4.1, §5.1 | T03 |
| 12. Instantánea de metadatos frente a auditoría | §4.3, §6.1 | T05, T06 |
| 13. Cambios con atributos conservados | §5.2 | T09 |
| 14. Archivos aún en copia | §5.4 | T10 |
| 15. Operaciones concurrentes | §6.3, §7.2 | T14 |
| 16. Cambio posterior a auditoría | §6.3, §6.4 | T13 |
| 17. Pérdida interna de eventos | §5.3 | T12 |
| 18. Dependencias externas | §5.5 | T16 |
| 19. Packs compartidos/asociaciones | §6.2 | T17 |
| 20. Reconexión e identidad | §5.4 | T11, T18 |
| 21. Almacenamiento/instancias | §7.2 | T14, T19 |
| 22. Evolución y restauración de DB | §8.3 | T20 |
| 23. Paginación concurrente | §8.1 | T21 |
| 24. Pack modificado durante partida | §6.4 | T22 |

## 12. Estado de implementación y límites actuales

Esta sección registra el avance; no reduce los requisitos anteriores ni declara aprobados T01–T24.

Implementado en el código:

- SQLite empaquetado, actor con cola acotada, WAL/FULL, escrituras por juego, importación transaccional del JSON conservando el original y verificando conteos, IDs y todos los valores importados, exclusión de escritores, rechazo de esquemas futuros, lectura de una DB sin permisos de escritura, comprobación de corrupción y rotación de tres copias verificadas al cierre.
- Inventario persistido por directorio y vigilancia `notify`: eventos agrupados a 250 ms con máximo de 1 s, reconciliación por pérdida de continuidad, identidad de raíz, conservación ante desconexión/error, exclusión de enlaces/reparse points y bajas demostradas mediante enumeraciones completas. Al reiniciar se reconcilian atributos; todavía no hay reproducción de un diario USN por volumen.
- CRC32/SHA-256 de ROM en una lectura cancelable; identidad lógica e historial, detección de reemplazos antes de jugar y estadísticas idempotentes por sesión. Abrir una ficha y lanzar comparten la preparación de ROM.
- Resumen pequeño de packs para asociaciones del catálogo; metadatos completos e inspecciones persistidas aparte. El catálogo no audita assets. La primera utilización de una revisión no confirmada inspecciona el pack seleccionado; las siguientes reutilizan el resultado mientras pueda confirmarse. La espera por una inspección existente puede cancelarse.
- Copia y auditoría por bloques de 1 MiB, límite de dos lectores pesados y uno por volumen, progreso/cancelación en instalación, ficha y preparación. Las reservas nativas impiden reemplazar un archivo durante su uso.
- Registro recuperable de la **promoción de instalaciones**: distingue archivo original, staging y archivo promovido; bloquea el pack pendiente y recupera conservadoramente sin atribuir vigencia a una auditoría tras una interrupción.
- Contrato de reservas entre Play y el checkout hermano `AYTHER-Runtime`: tokens `--rom-revision`/`--pack-revision` y confirmación `AYTHER_RESERVATION 1`. Runtime conserva sus handles durante la sesión. Se verificó el intercambio real de tokens y una prueba nativa independiente de bloqueo de escritura/borrado/renombrado.
- Portadas resueltas desde el inventario, decodificación en trabajador, carga a demanda, grilla virtualizada, presupuesto de caché de imágenes y descarte de resultados de imágenes de generaciones anteriores. Un escaneo sin cambios no reconstruye el modelo ni reabre la ficha.

Pendiente para completar este plan:

1. Paginación **SQL** de 60 filas y generaciones de catálogo, eliminación de la materialización completa de `Library`/`all_items`, y pruebas de selección/ancla durante cambios concurrentes. La virtualización y carga diferida de imágenes actuales no equivalen a esa paginación.
2. Recuperación transaccional completa de **desinstalación y rollback**, actualización atómica de todos los vínculos de packs y recuperación independiente de cada operación. El registro recuperable actual cubre la promoción, no todo el ciclo de vida.
3. Referencias de consumidores para una tarea compartida y prioridad del uso interactivo: hoy se serializan inspecciones del mismo path; cancelar el primer consumidor puede obligar al siguiente a reintentar. No se declara implementada todavía la política completa de §6.3.
4. Activación de la primera ventana desde una segunda instancia; restauración de DB mediante una acción visible; copias previas a futuras migraciones; control explícito de WAL a 64/256 MiB y retención de datos derivados por antigüedad.
5. Invalidación completa por DAT/core/Runtime/reglas/confianza, aliases e identidades entre ubicaciones, estados separados de disponibilidad/vigencia/compatibilidad y recuperación guiada cuando cambia la identidad de una raíz. El cambio de DAT cargado actualiza nombres desde CRC sin leer ROMs, pero aún falta vigilar todos esos archivos externos.
6. Eliminar las operaciones de disco que todavía quedan en callbacks de configuración, guardados y creación de sesión; verificar navegación con mando, progreso terminal y maximización desde el primer fotograma en la aplicación real.
7. Ejecutar la matriz T01–T24 completa, inyección de fallos y las series de 30 arranques calientes/10 fríos, p95 de UI/DB, memoria y WAL de §10. Las pruebas unitarias, el benchmark previo y una compilación exitosa **no** demuestran estos presupuestos.

Ver [VALIDACION-CACHE-INCREMENTAL.md](VALIDACION-CACHE-INCREMENTAL.md) para comandos y evidencia. La compilación de esta etapa es una vista previa de desarrollo, no una entrega con aceptación completa de este plan.
