// GENERADO por play-ce/tools/gen_translations.py — NO editar a mano.
//
// El catálogo de los textos que arma Rust (`tr!`). Sale del mismo
// diccionario que los .po que Slint empaqueta, así la UI y los mensajes
// de estado no pueden hablar idiomas distintos.

/// Traducción de `msgid` al idioma `lang`, o `None` si no hay (el
/// llamador cae al msgid, que está en español: el idioma base).
pub fn lookup(lang: &str, msgid: &str) -> Option<&'static str> {
    match lang {
        "en" => match msgid {
            "Jugar original" => Some("Play original"),
            "Actualizá el motor de juego" => Some("Update the game engine"),
            "Este Runtime no confirmó los archivos preparados. Instalá la compilación de Runtime que acompaña a esta versión de Play CE y volvé a intentar." => {
                Some(
                    "This Runtime did not confirm the prepared files. Install the Runtime build supplied with this version of Play CE and try again.",
                )
            }
            "No se pudo reservar el archivo" => Some("Could not reserve the file"),
            "La ROM o el pack cambió o está en uso. Esperá a que termine la copia o modificación y volvé a intentar." => {
                Some(
                    "The ROM or pack changed or is in use. Wait for the copy or modification to finish and try again.",
                )
            }
            "Cargando biblioteca…" => Some("Loading library…"),
            "Biblioteca cargada" => Some("Library loaded"),
            "Biblioteca lista. Revisá en Ajustes las carpetas que no están disponibles." => {
                Some("Library ready. Check unavailable folders in Settings.")
            }
            "Copiando pack…" => Some("Copying pack…"),
            "Cancelando…" => Some("Cancelling…"),
            "Preparando juego…" => Some("Preparing game…"),
            "Comprobando pack…" => Some("Checking pack…"),
            "Identificando juegos…" => Some("Identifying games…"),
            "Cargando carátulas…" => Some("Loading covers…"),
            "Buscando juegos…" => Some("Finding games…"),
            "Buscando packs…" => Some("Finding packs…"),
            "Comprobando compatibilidad…" => Some("Checking compatibility…"),
            "Buscando carátulas…" => Some("Finding covers…"),
            "No se reconoce la firma del pack" => Some("The pack signature is not recognized"),
            "El pack está firmado, pero falta registrar su certificado de confianza en este equipo. Podés jugar sin el pack o configurar el certificado público de su autor." => {
                Some(
                    "The pack is signed, but its trusted certificate has not been registered on this computer. You can play without the pack or configure its author's public certificate.",
                )
            }
            "No se pudo cargar el pack" => Some("The pack could not be loaded"),
            "El sistema rechazó el pack al comprobarlo. Volvé a instalarlo desde una copia válida o elegí otro. También podés jugar al original sin el pack." => {
                Some(
                    "The system rejected the pack during verification. Reinstall it from a valid copy or choose another one. You can also play the original game without the pack.",
                )
            }
            "No se pudo leer el certificado del pack" => {
                Some("The pack certificate could not be read")
            }
            "El archivo de certificados configurado no existe, no se puede leer o tiene un formato inválido. Revisá su ubicación. Podés jugar sin el pack mientras lo corregís." => {
                Some(
                    "The configured certificate file is missing, unreadable, or invalid. Check its location. You can play without the pack while you fix it.",
                )
            }
            "No se pudieron cargar los controles" => Some("The controls could not be loaded"),
            "No se pudo cargar el core" => Some("The core could not be loaded"),
            "El emulador elegido no se pudo abrir. Revisá el core de esta consola en Ajustes y comprobá que sus archivos estén disponibles." => {
                Some(
                    "The selected emulator could not be opened. Check this console's core in Settings and make sure its files are available.",
                )
            }
            "No se pudo iniciar el juego" => Some("The game could not be started"),
            "No se pudo iniciar el motor de juego. Revisá que AYTHER Runtime esté instalado y sea compatible con esta versión de Play." => {
                Some(
                    "The game engine could not be started. Check that AYTHER Runtime is installed and compatible with this version of Play.",
                )
            }
            "El juego no llegó a arrancar. El diagnóstico contiene los detalles del fallo. Podés probar el modo seguro, que desactiva las mejoras." => {
                Some(
                    "The game did not start. The diagnostic report contains details about the failure. You can try safe mode, which disables enhancements.",
                )
            }
            "La partida se interrumpió. Podés intentar reiniciarla en modo seguro; la ROM y los guardados existentes se conservan." => {
                Some(
                    "The game was interrupted. You can try restarting in safe mode; your ROM and existing saves are preserved.",
                )
            }
            "Jugar sin pack" => Some("Play without pack"),
            "Cambiar pack" => Some("Switch pack"),
            "Desinstalar pack" => Some("Uninstall pack"),
            "Seleccionado" => Some("Selected"),
            "↑ Instalar pack · → Cambiar pack · ↓ Desinstalar pack" => {
                Some("↑ Install pack · → Switch pack · ↓ Uninstall pack")
            }
            "¿Desinstalar este pack?" => Some("Uninstall this pack?"),
            "Se elimina el archivo del pack seleccionado y, si fue instalado por Play, su versión anterior. Tus ROMs, cores, guardados y otros packs se conservan." => {
                Some(
                    "The selected pack file is deleted, along with its previous version if installed by Play. Your ROMs, cores, saves and other packs are preserved.",
                )
            }
            "Pack desinstalado." => Some("Pack uninstalled."),
            "El pack ya no está en el disco." => Some("The pack is no longer on disk."),
            "No se pudo desinstalar el pack: {}" => Some("Could not uninstall the pack: {}"),
            "Asignar carátula" => Some("Assign cover art"),
            "ELEGÍ UNA CARÁTULA" => Some("CHOOSE COVER ART"),
            "Elegí una imagen PNG, JPG o WebP." => Some("Choose a PNG, JPG or WebP image."),
            "No se pudo abrir la imagen. Elegí otra carátula." => {
                Some("The image could not be opened. Choose another cover.")
            }
            "El juego ya no está en la biblioteca." => {
                Some("The game is no longer in the library.")
            }
            "El mapa de controles no es válido. Revisá Ajustes > Controles del juego." => {
                Some("The control map is invalid. Check Settings > Game controls.")
            }
            "Espacio" => Some("Space"),
            "Retroceso" => Some("Backspace"),
            "Enter" => Some("Enter"),
            "Re Pág" => Some("Pg Up"),
            "Av Pág" => Some("Pg Dn"),
            "Falta el core" => Some("Core missing"),
            "El core no sirve" => Some("The core does not work"),
            "Compatible" => Some("Compatible"),
            "Con advertencias" => Some("With warnings"),
            "Incompatible" => Some("Incompatible"),
            "No verificado" => Some("Not verified"),
            "Escaneando la biblioteca" => Some("Scanning the library"),
            "Escaneando… {} juego encontrado" => Some("Scanning… {} game found"),
            "Escaneando… {} juegos encontrados" => Some("Scanning… {} games found"),
            "Cancelar" => Some("Cancel"),
            "No hay ROMs — apretá F5 para escanear" => Some("No ROMs found — press F5 to scan"),
            "Activo" => Some("On"),
            "Apagado" => Some("Off"),
            "No se puede usar" => Some("Can't be used"),
            "Instalar un pack desde el disco" => Some("Install a pack from disk"),
            "Desactivar" => Some("Turn off"),
            "Activar" => Some("Turn on"),
            "Cambiar" => Some("Change"),
            "Jugar" => Some("Play"),
            "Atrás" => Some("Back"),
            "Sección" => Some("Section"),
            "Elegir" => Some("Choose"),
            "Entrar" => Some("Sign in"),
            "Usar esta carpeta" => Some("Use this folder"),
            "Cerrar" => Some("Close"),
            "Revertir" => Some("Revert"),
            "Tecla" => Some("Key"),
            "Botón" => Some("Button"),
            "Restaurar todo" => Some("Reset all"),
            "Restaurar" => Some("Restore"),
            "Activar/Desactivar pack" => Some("Turn pack on/off"),
            "Partida nueva" => Some("New game"),
            "Modo seguro" => Some("Safe mode"),
            "Recuperar" => Some("Recover"),
            "Descartar" => Some("Discard"),
            "Puntuar" => Some("Rate"),
            "Guardar" => Some("Save"),
            "Ahora no" => Some("Not now"),
            "Continuar" => Some("Continue"),
            "Ficha" => Some("Details"),
            "Menú" => Some("Menu"),
            "Seleccionar" => Some("Select"),
            "Filtrar" => Some("Filter"),
            "Orden: Z → A" => Some("Sort: Z → A"),
            "Orden: por consola" => Some("Sort: by system"),
            "Orden: recientes" => Some("Sort: recently played"),
            "Orden: A → Z" => Some("Sort: A → Z"),
            "Secciones de Play" => Some("Play sections"),
            "INICIO" => Some("HOME"),
            "BIBLIOTECA" => Some("LIBRARY"),
            "AJUSTES" => Some("SETTINGS"),
            "En juego" => Some("Playing"),
            "Escaneando…" => Some("Scanning…"),
            "CONTINUAR" => Some("CONTINUE"),
            "Ver biblioteca" => Some("View library"),
            "TODAVÍA NO HAY JUEGOS" => Some("NO GAMES YET"),
            "EMPEZÁ POR DONDE QUIERAS" => Some("START ANYWHERE"),
            "Agregá una carpeta con tus ROMs desde Biblioteca o Ajustes › Rutas." => {
                Some("Add a folder with your ROMs from Library or Settings › Paths.")
            }
            "Cuando juegues algo, va a aparecer acá para seguir de una." => {
                Some("Once you play something it shows up here, ready to resume.")
            }
            "Ir a Biblioteca" => Some("Go to Library"),
            "RECIENTES" => Some("RECENT"),
            "Buscar juego…" => Some("Search games…"),
            "Todas" => Some("All"),
            "Compatibles" => Some("Compatible"),
            "Favoritos" => Some("Favourites"),
            "Ordenar la biblioteca" => Some("Sort the library"),
            "Vista de grilla" => Some("Grid view"),
            "Vista de lista" => Some("List view"),
            "{} de {} juegos" => Some("{} of {} games"),
            "Quitar filtros" => Some("Clear filters"),
            "SISTEMA" => Some("SYSTEM"),
            "Idioma" => Some("Language"),
            "Predeterminado · Español · English" => Some("Default · Español · English"),
            "global" => Some("global"),
            "Carpetas de ROMs" => Some("ROM folders"),
            "Editar" => Some("Edit"),
            "Cores" => Some("Cores"),
            "Administrar" => Some("Manage"),
            "Perfil de salida" => Some("Output profile"),
            "Cómo se ve la imagen: CRT, LCD, píxel exacto…" => {
                Some("How the image looks: CRT, LCD, pixel-perfect…")
            }
            "Controles del juego" => Some("Game controls"),
            "Configurar" => Some("Configure"),
            "Carpeta de packs" => Some("Pack folder"),
            "Un .ay que conseguiste por fuera: se audita entero y la versión anterior queda guardada" => {
                Some(
                    "An .ay you got elsewhere: it is audited in full and the previous version is kept",
                )
            }
            "Instalar" => Some("Install"),
            "Carpeta de datos" => Some("Data folder"),
            "Exportar mis datos" => Some("Export my data"),
            "Un ZIP con tus guardados y tus preferencias, para llevártelo" => {
                Some("A ZIP with your saves and preferences, to take with you")
            }
            "Exportar" => Some("Export"),
            "Volver a ver la bienvenida" => Some("Show the welcome again"),
            "Onboarding y términos BYOR/BYOC" => Some("Onboarding and BYOR/BYOC terms"),
            "Abrir" => Some("Open"),
            "↑↓ elegir   A cambiar   ←→ sección   B atrás" => {
                Some("↑↓ move   A change   ←→ section   B back")
            }
            "Configuración" => Some("Settings"),
            "Guardados" => Some("Saves"),
            "Tiempo jugado" => Some("Time played"),
            "Última vez" => Some("Last played"),
            "Consola" => Some("System"),
            "ESTADO" => Some("STATUS"),
            "PACK" => Some("PACK"),
            "No hay pack para este juego — se juega el original." => {
                Some("No pack for this game — the original is played.")
            }
            "Sin pack: se juega el original, completo." => {
                Some("No pack: you play the original, in full.")
            }
            "Un pack HD es un archivo .ay que conseguís por fuera. Instalalo desde acá, o dejalo en tu carpeta de packs y aparece solo." => {
                Some(
                    "An HD pack is an .ay file you get elsewhere. Install it from here, or drop it in your pack folder and it shows up on its own.",
                )
            }
            "El mando lo maneja el juego" => Some("The game owns the controller"),
            "En el juego: F2 compara con el original    F12 saca la captura" => {
                Some("In game: F2 compares with the original    F12 takes a capture")
            }
            "Agregá ROMs a tu biblioteca" => Some("Add ROMs to your library"),
            "Quitar de favoritos" => Some("Remove from favourites"),
            "Favorito" => Some("Favourite"),
            "Elegir pack" => Some("Choose pack"),
            "Elegir core" => Some("Choose core"),
            "Editar metadatos" => Some("Edit metadata"),
            "Restaurar valores automáticos" => Some("Restore automatic values"),
            "Mostrar" => Some("Show"),
            "Ocultar" => Some("Hide"),
            "Detectado: {}" => Some("Detected: {}"),
            "GUARDADOS" => Some("SAVES"),
            "Todavía no hay ninguna partida guardada de este juego." => {
                Some("There is no saved game for this title yet.")
            }
            "Actual" => Some("Current"),
            "Anterior" => Some("Earlier"),
            "Restaurar no borra nada" => Some("Restoring deletes nothing"),
            "La versión que estás jugando se conserva en el historial, con su fecha. La ROM no se modifica." => {
                Some(
                    "The version you are playing stays in the history, with its date. The ROM is not modified.",
                )
            }
            "CONTROLES DEL JUEGO" => Some("GAME CONTROLS"),
            "Qué hace cada tecla y cada botón dentro de la partida. La cruceta no se cambia: es dirección." => {
                Some(
                    "What each key and each button does inside the game. The d-pad is not remappable: it is direction.",
                )
            }
            "Acción" => Some("Action"),
            "apretá una tecla…" => Some("press a key…"),
            "apretá un botón…" => Some("press a button…"),
            "Esperando una tecla del teclado — Esc cancela" => {
                Some("Waiting for a keyboard key — Esc cancels")
            }
            "Esperando un botón del mando — Esc cancela" => {
                Some("Waiting for a gamepad button — Esc cancels")
            }
            "ELEGÍ TU CARPETA DE PACKS" => Some("CHOOSE YOUR PACK FOLDER"),
            "INSTALAR UN PACK DESDE EL DISCO" => Some("INSTALL A PACK FROM DISK"),
            "Acá no hay ningún pack: se buscan archivos .ay." => {
                Some("No packs here: this looks for .ay files.")
            }
            "Auditando e instalando… El archivo se comprueba entero antes de tocar nada." => {
                Some(
                    "Auditing and installing… The file is checked in full before anything is touched.",
                )
            }
            "La auditoría dice que el archivo está completo, no de quién es: eso CE no puede saberlo." => {
                Some(
                    "The audit says the file is complete, not who it is from: CE cannot know that.",
                )
            }
            "EL JUEGO SE CERRÓ INESPERADAMENTE" => Some("THE GAME CLOSED UNEXPECTEDLY"),
            "Reiniciar en modo seguro" => Some("Restart in safe mode"),
            "Se lanza sin pack, sin shaders y sin overrides. La ROM y tus guardados quedan intactos." => {
                Some(
                    "It starts with no pack, no shaders and no overrides. Your ROM and your saves are left untouched.",
                )
            }
            "SESIÓN SIN CERRAR" => Some("UNFINISHED SESSION"),
            "Descartar borra sólo el registro de esa sesión. La ROM, la SRAM y tus guardados no se tocan." => {
                Some(
                    "Discarding only deletes that session's record. Your ROM, its SRAM and your saves are left untouched.",
                )
            }
            "Modo seguro: sin pack, shaders ni overrides. La ROM y los guardados no se tocan." => {
                Some(
                    "Safe mode: no pack, no shaders, no overrides. The ROM and the saves are left untouched.",
                )
            }
            "Salir del modo seguro" => Some("Leave safe mode"),
            "PREPARANDO…" => Some("GETTING READY…"),
            "PREPARANDO {}…" => Some("GETTING {} READY…"),
            "Mientras jugás, Play se queda esperando acá. Cuando cierres el juego, volvés a esta pantalla." => {
                Some(
                    "While you play, Play waits here. When you close the game, you come back to this screen.",
                )
            }
            "CONFIGURACIÓN PREVIA" => Some("BEFORE YOU PLAY"),
            "Sin pack — se juega el original" => Some("No pack — the original is played"),
            "Pack: {}" => Some("Pack: {}"),
            "Volver" => Some("Go back"),
            "D / Esc / B volver" => Some("D / Esc / B back"),
            "Créditos" => Some("Credits"),
            "Este pack no declara créditos." => Some("This pack declares no credits."),
            "C / Esc / B volver" => Some("C / Esc / B back"),
            "SESIÓN TERMINADA" => Some("SESSION ENDED"),
            "Esta sesión" => Some("This session"),
            "Capturas" => Some("Screenshots"),
            "¿Cómo quedó este pack?" => Some("How did this pack turn out?"),
            "Se guarda en tu equipo. No se envía nada sin que vos lo pidas." => {
                Some("Stored on your machine. Nothing is sent unless you ask for it.")
            }
            "↑↓ elegir    ←→ puntuar    Enter guardar    Esc ahora no    N no preguntar más" => {
                Some("↑↓ move    ←→ rate    Enter save    Esc not now    N stop asking")
            }
            "Paso {} de {}" => Some("Step {} of {}"),
            "Movete con el stick o la cruceta y confirmá con A. Si no se detecta ningún mando, podés seguir con teclado." => {
                Some(
                    "Move with the stick or the D-pad and confirm with A. If no controller is detected, you can continue with the keyboard.",
                )
            }
            "Ningún mando detectado — podés seguir con teclado" => {
                Some("No controller detected — you can carry on with the keyboard")
            }
            "conectado" => Some("connected"),
            "Play sólo lee. No copia, no mueve y no sube nada." => {
                Some("Play only reads. It never copies, moves or uploads anything.")
            }
            "Quitar la carpeta" => Some("Remove the folder"),
            "Agregar carpeta" => Some("Add folder"),
            "Elegí el archivo del core que vas a usar." => {
                Some("Pick the core file you are going to use.")
            }
            "Acepto los términos" => Some("I accept the terms"),
            "Entiendo y acepto" => Some("I understand and accept"),
            "Aceptado el {}" => Some("Accepted on {}"),
            "Arriba" => Some("Up"),
            "Abajo" => Some("Down"),
            "Izquierda" => Some("Left"),
            "Derecha" => Some("Right"),
            "Shift der." => Some("Right Shift"),
            "Shift izq." => Some("Left Shift"),
            "no es una DLL de Windows" => Some("not a Windows DLL"),
            "el runtime no informó el resultado del sondeo" => {
                Some("the runtime did not report the probe result")
            }
            "Core válido: {} {}" => Some("Core is valid: {} {}"),
            "No se encontró el core" => Some("Core not found"),
            "El core es de otra arquitectura" => Some("The core is built for another architecture"),
            "Ese archivo no es un core" => Some("That file is not a core"),
            "El core usa una versión de libretro que no soportamos" => {
                Some("The core uses a libretro version we do not support")
            }
            "No se pudo comprobar el core" => Some("The core could not be checked"),
            "No declara extensiones conocidas; se aceptará igual, pero puede rechazar tus ROM." => {
                Some(
                    "It declares no known file extensions; it will be accepted, but it may reject your ROMs.",
                )
            }
            "Sirve para: {}" => Some("Works with: {}"),
            "Elegí el archivo del core en tu equipo. Buscábamos: {}" => {
                Some("Pick the core file on your computer. We looked for: {}")
            }
            "Descargá la versión {} del core: la que elegiste es {}." => {
                Some("Download the {} build of the core: the one you picked is {}.")
            }
            "Elegí un core de libretro (por ejemplo genesis_plus_gx_libretro), no otra DLL." => {
                Some(
                    "Pick a libretro core (genesis_plus_gx_libretro, for example), not another DLL.",
                )
            }
            "Este core habla la API {} y Ayther habla la 1. Actualizá el core." => {
                Some("This core speaks API {} and Ayther speaks 1. Update the core.")
            }
            "Revisá la instalación de Ayther. Detalle: {}" => {
                Some("Check your Ayther installation. Details: {}")
            }
            "no se pudo ejecutar {}: {}" => Some("could not run {}: {}"),
            "el sistema no pudo cargar la DLL; puede faltarle una dependencia" => {
                Some("the system could not load the DLL; it may be missing a dependency")
            }
            "El core configurado corre {}, no {}. Elegí uno en Ajustes." => {
                Some("The configured core runs {}, not {}. Pick one in Settings.")
            }
            "Jugando: {}" => Some("Playing: {}"),
            "Instalado: {}" => Some("Installed: {}"),
            "El pack quedó instalado y auditado: completo y sin alterar. Aparece en el juego que declara, con la biblioteca ya al día." => {
                Some(
                    "The pack is installed and audited: complete and unaltered. It shows up on the game it declares, with the library already refreshed.",
                )
            }
            "Actualizado: {}" => Some("Updated: {}"),
            "El pack nuevo quedó instalado y auditado. La versión anterior quedó guardada: podés volver a ella desde acá o cuando quieras." => {
                Some(
                    "The new pack is installed and audited. The previous version was kept: you can go back to it from here, or any time.",
                )
            }
            "No se instaló" => Some("Not installed"),
            "El archivo no pasó la auditoría: {}. Lo que tenías instalado sigue intacto." => {
                Some("The file failed the audit: {}. What you had installed is untouched.")
            }
            "No se pudo instalar" => Some("Could not install"),
            "Falló el disco, no el pack: {}. Lo que tenías instalado sigue intacto." => {
                Some("The disk failed, not the pack: {}. What you had installed is untouched.")
            }
            "Se volvió a la versión anterior" => Some("Back on the previous version"),
            "La que estaba quedó guardada a su vez: revertir INTERCAMBIA, no borra, así que se puede volver a adelantar." => {
                Some(
                    "The one you had was kept in turn: reverting SWAPS, it does not delete, so you can move forward again.",
                )
            }
            "No se pudo revertir" => Some("Could not revert"),
            "{}. El pack instalado no se tocó." => Some("{}. The installed pack was not touched."),
            "{} juego en la biblioteca" => Some("{} game in library"),
            "{} juegos en la biblioteca" => Some("{} games in library"),
            "La última sesión de {} ({}) no registró su cierre. Play encontró su manifest." => {
                Some("The last {} session ({}) never recorded its exit. Play found its manifest.")
            }
            "Savestate de salida" => Some("Exit savestate"),
            "1 carpeta" => Some("1 folder"),
            "{} carpetas" => Some("{} folders"),
            "1 juego" => Some("1 game"),
            "{} juegos" => Some("{} games"),
            "1 core" => Some("1 core"),
            "{} cores" => Some("{} cores"),
            "ninguna consola cubierta" => Some("no system covered"),
            "Automático" => Some("Automatic"),
            "De fábrica: flechas + Z X A S · el mando tal cual" => {
                Some("Factory: arrows + Z X A S; the gamepad as-is")
            }
            "Personalizados" => Some("Customized"),
            "Sin carpeta — valen los instalados y los que estén junto a la ROM" => {
                Some("No folder — installed packs and those next to the ROM still count")
            }
            "1 pack" => Some("1 pack"),
            "{} packs" => Some("{} packs"),
            "Listo: {} guardado(s) en {}" => Some("Done: {} save(s) in {}"),
            "No se pudo exportar" => Some("Couldn't export"),
            "La carpeta dejó de mirarse. Tus packs quedan donde están; los instalados no cambian." => {
                Some(
                    "The folder is no longer watched. Your packs stay where they are; installed ones do not change.",
                )
            }
            "Sesión recuperada: {}" => Some("Session recovered: {}"),
            "Nunca" => Some("Never"),
            "sin revisión" => Some("no revision"),
            "Rev {}" => Some("Rev {}"),
            "Guardado {}" => Some("Saved {}"),
            "Leyendo el pack…" => Some("Reading the pack…"),
            "sin autor declarado" => Some("no author declared"),
            "Restaurado el guardado de {}" => Some("Restored the save from {}"),
            "No se pudo restaurar el guardado." => Some("The save could not be restored."),
            "{} ya está en juego (pid {})" => Some("{} is already running (pid {})"),
            "Partida preparada" => Some("Game ready"),
            "No se pudo preparar la partida" => Some("Couldn't get the game ready"),
            "Abriendo el juego…" => Some("Opening the game…"),
            "Esperando a que arranque…" => Some("Waiting for it to start…"),
            "No se pudo jugar: {}" => Some("Could not play: {}"),
            "Si vuelve a fallar, el diagnóstico redactado (sin rutas ni nombre de usuario) queda en {}." => {
                Some(
                    "If it fails again, the redacted diagnostic (no paths, no user name) is kept in {}.",
                )
            }
            "No se pudo escribir el diagnóstico de la sesión." => {
                Some("The session diagnostic could not be written.")
            }
            "{} s" => Some("{} s"),
            "{} min" => Some("{} min"),
            "ninguno" => Some("none"),
            "SRAM" => Some("SRAM"),
            "1 estado" => Some("1 state"),
            "{} estados" => Some("{} states"),
            "SRAM + 1 estado" => Some("SRAM + 1 state"),
            "SRAM + {} estados" => Some("SRAM + {} states"),
            "{} min jugados" => Some("{} min played"),
            "Sin ROM conocidas" => Some("No known ROMs"),
            "{} archivos" => Some("{} files"),
            "con pack HD" => Some("with HD pack"),
            "Jugado:" => Some("Played:"),
            "este juego" => Some("this game"),
            "esta consola" => Some("this system"),
            "el pack" => Some("the pack"),
            "de fábrica" => Some("factory default"),
            "Escaneo cancelado — {} juegos en la biblioteca" => {
                Some("Scan cancelled — {} games in library")
            }
            "{} juego encontrado" => Some("{} game found"),
            "{} juegos encontrados" => Some("{} games found"),
            "AYTHER no distribuye ROMs ni cores, y no ayuda a conseguirlos: no incluye ninguno, no los descarga y no enlaza a ningún sitio que los ofrezca." => {
                Some(
                    "AYTHER does not distribute ROMs or cores, and does not help you get them: it ships none, downloads none, and links to no site that offers them.",
                )
            }
            "BYOR — la ROM la ponés vos. Play lee los archivos de las carpetas que le indiques y nunca los copia, los mueve ni los sube. Tener derecho a usar esas copias es tuyo." => {
                Some(
                    "BYOR — you bring the ROM. Play reads the files in the folders you point it at, and never copies, moves or uploads them. Holding the right to use those copies is on you.",
                )
            }
            "BYOC — el core lo ponés vos. El emulador que ejecuta el juego es un programa de terceros que elegís e instalás vos; Play lo lanza en un proceso aparte y no lo modifica." => {
                Some(
                    "BYOC — you bring the core. The emulator that runs the game is third-party software you choose and install yourself; Play launches it in a separate process and never modifies it.",
                )
            }
            "Un pack de remasterización no contiene partes del juego original. Aporta material propio de su autor —gráficos, sonido, video— que se aplica sobre tu copia mientras jugás: sin tu ROM no hay nada que jugar." => {
                Some(
                    "A remaster pack contains no part of the original game. It brings its author’s own material — art, sound, video — applied over your copy while you play: without your ROM there is nothing to play.",
                )
            }
            "Nada de esto sale de tu máquina. En modo local, Play no envía tu biblioteca, tus guardados ni qué estás jugando." => {
                Some(
                    "None of this leaves your machine. In local mode, Play sends neither your library, nor your saves, nor what you are playing.",
                )
            }
            "Tu biblioteca retro, remasterizada. Vos ponés la ROM y nosotros el resto." => {
                Some("Your retro library, remastered. You bring the ROM, we do the rest.")
            }
            "PROBÁ TU MANDO" => Some("TEST YOUR CONTROLLER"),
            "¿DÓNDE ESTÁN TUS JUEGOS?" => Some("WHERE ARE YOUR GAMES?"),
            "ELEGÍ TU CORE" => Some("PICK YOUR CORE"),
            "TÉRMINOS DE USO" => Some("TERMS OF USE"),
            "TODO LISTO" => Some("ALL SET"),
            "Español" => Some("Spanish"),
            "English" => Some("English"),
            "Predeterminado" => Some("Default"),
            "Agregá al menos una carpeta con tus ROM." => {
                Some("Add at least one folder with your ROMs.")
            }
            "Marcá que entendés que las ROM y el core los ponés vos." => {
                Some("Tick the box to confirm you bring the ROMs and the core yourself.")
            }
            "Quitar" => Some("Remove"),
            "Aceptar" => Some("Accept"),
            "Comenzar" => Some("Start"),
            "Agregar" => Some("Add"),
            "Elegir otro" => Some("Choose another"),
            "Salir" => Some("Exit"),
            "{} · {} juegos" => Some("{} · {} games"),
            "Sin core" => Some("No core"),
            "Carpetas" => Some("Folders"),
            "Core" => Some("Core"),
            "Completo y sin alterar" => Some("Complete and unaltered"),
            "No trae índice: no se puede auditar" => Some("No index: it cannot be audited"),
            "El archivo no cierra" => Some("The file does not add up"),
            "No es un pack .ay" => Some("Not an .ay pack"),
            _ => None,
        },
        _ => None,
    }
}
