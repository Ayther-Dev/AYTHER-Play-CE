//! El foco de la UI y el ruteo del mando cuentan la misma historia (#586).
//!
//! `pad_router::superficie()` (Rust) decide **qué hace** cada botón, y la
//! propiedad derivada `pad-surface` del `.slint` decide **qué botones se
//! anuncian** en el pie de pantalla. Son dos listas con la misma precedencia
//! escritas en dos lenguajes: si divergen, los hints mandan al jugador a
//! probar teclas que no hacen nada — y eso, en una TV y con un mando, se lee
//! como «la aplicación se colgó».
//!
//! Estos tests no abren una ventana: leen el `.slint` como texto. Es lo único
//! que puede correr en el CI de Linux sin display.

use std::path::PathBuf;

fn ui() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/appwindow.slint");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}
fn router() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/pad_router.rs");
    std::fs::read_to_string(&p).unwrap()
}

/// El orden de la cadena `pad-surface` es el de `pad_router::superficie()`.
#[test]
fn surface_precedence_is_the_same_in_the_ui_and_in_the_router() {
    let ui = ui();
    let start = ui
        .find("out    property <string>         pad-surface:")
        .expect("la UI no declara `pad-surface` como propiedad derivada");
    let end = ui[start..]
        .find(';')
        .expect("la cadena de pad-surface no termina")
        + start;
    let surface_expression = &ui[start..end];

    // Las claves salen de `hints()`, que es un `match` sobre TODAS las
    // superficies y por lo tanto exhaustivo por el compilador.
    //
    // EL DEFECTO QUE ESTO CIERRA: antes la lista estaba escrita a mano acá y
    // además se FILTRABA por «lo que aparece en la UI», así que una superficie
    // nueva que nadie hubiera puesto en la cadena de la UI no rompía nada — el
    // test la ignoraba y seguía en verde. Un oráculo que se recorta a lo que
    // encuentra no prueba nada sobre lo que falta.
    let rs = router();
    let keys = hint_keys(&rs);
    assert!(
        keys.len() >= 12,
        "hints() dejó de enumerar las superficies: {keys:?}"
    );

    let mut ordered_ui: Vec<(usize, &str)> = keys
        .iter()
        .map(|k| {
            let pos = surface_expression
                .find(&format!("\"{k}\""))
                .unwrap_or_else(|| {
                    panic!(
                        "la cadena `pad-surface` de la UI no contempla la superficie «{k}».\n\
                 Toda superficie del router tiene que poder ganar el foco, o el pie \n\
                 de pantalla va a anunciar los botones de otra pantalla."
                    )
                });
            (pos, k.as_str())
        })
        .collect();
    ordered_ui.sort();
    let ui_order: Vec<&str> = ordered_ui.into_iter().map(|(_, k)| k).collect();

    // Claves en el orden en que `hints()` las devuelve, que es el del `match`
    // de `superficie()`.
    let start = rs
        .find("pub fn hints(")
        .expect("pad_router no expone hints()");
    let body = &rs[start..];
    let mut ordered_rust: Vec<(usize, &str)> = ui_order
        .iter()
        .map(|k| {
            let pat = format!("=> \"{k}\"");
            (
                body.find(&pat)
                    .unwrap_or_else(|| panic!("pad_router::hints() no devuelve «{k}»")),
                *k,
            )
        })
        .collect();
    ordered_rust.sort();
    let rust_order: Vec<&str> = ordered_rust.into_iter().map(|(_, k)| k).collect();

    assert_eq!(
        ui_order, rust_order,
        "la precedencia de superficies difiere entre appwindow.slint y pad_router.rs.\n\
         UI:     {ui_order:?}\n\
         router: {rust_order:?}"
    );
    assert_eq!(
        ui_order.len(),
        keys.len(),
        "faltan superficies en la cadena de la UI"
    );
}

/// Las claves de superficie, en el orden en que `hints()` las escribe.
///
/// `hints()` es un `match` sobre `Superficie`: el compilador obliga a que estén
/// todas, así que leerlo es la única forma de que este archivo no tenga su
/// propia lista — que es como un oráculo se queda ciego a lo que se agrega
/// después de escribirlo.
fn hint_keys(rs: &str) -> Vec<String> {
    let start = rs
        .find("pub fn hints(")
        .expect("pad_router no expone hints()");
    let body = &rs[start..];
    let end = body.find("\n}").map(|i| i + start).unwrap_or(rs.len());
    let mut v = Vec::new();
    let mut remaining = &rs[start..end];
    while let Some(i) = remaining.find("=> \"") {
        remaining = &remaining[i + 4..];
        let j = remaining
            .find('"')
            .expect("una clave sin cerrar en hints()");
        v.push(remaining[..j].to_string());
        remaining = &remaining[j..];
    }
    v
}

/// Cada superficie anuncia botones. Un pie vacío deja al jugador adivinando.
#[test]
fn every_surface_announces_its_buttons() {
    let ui = ui();
    let footer = {
        let i = ui
            .find("root.pad-surface ==")
            .expect("el pie no mira pad-surface");
        &ui[i..(i + 4000).min(ui.len())]
    };
    // Las secciones de fondo (`inicio`, `biblioteca`) son el caso por defecto
    // del pie y no aparecen nombradas; todo lo que TAPA algo, sí — porque es
    // ahí donde el jugador no sabe con qué salir.
    let rs = router();
    for key in hint_keys(&rs) {
        if matches!(key.as_str(), "inicio" | "biblioteca") {
            continue;
        }
        assert!(
            footer.contains(&format!("\"{key}\"")),
            "el pie de pantalla no dice qué botones usa la superficie «{key}»"
        );
    }
}

/// La ficha de juego vive DENTRO del chasis, entre las dos barras.
///
/// Estaba declarada afuera del chasis, así que se dibujaba encima de todo a
/// pantalla completa y se llevaba puestas la TopBar y el pie: sin TopBar no
/// había cómo volver a la biblioteca con el mouse, y sin pie no había quién
/// dijera que «B» vuelve. Una pantalla sin salida visible es una pantalla de la
/// que se sale cerrando la aplicación.
///
/// El orden del archivo ES el orden del árbol: que la ficha esté entre la
/// TopBar y el pie significa que es una sección más, como la biblioteca.
/// #615 — y la del PACK también. Se entregó una vez fuera del chasis y se veía
/// exactamente el defecto que este oráculo describe: sin TopBar y sin pie, o
/// sea sin nadie diciendo que «B» vuelve.
#[test]
fn the_cards_live_inside_the_chassis() {
    let ui = ui();
    let topbar = ui.find("// ---- TopBar h96").expect("no está la TopBar");
    let footer = ui.find("// ---- ButtonHints h72").expect("no está el pie");
    {
        let (kind, anchor) = ("la ficha de juego", "if root.details-open");
        let pos = ui.find(anchor).unwrap_or_else(|| panic!("no está {kind}"));
        assert!(
            topbar < pos && pos < footer,
            "{kind} se dibuja fuera del chasis: vuelve a tapar la TopBar y el pie"
        );
    }
}

/// La lista del explorador de carpetas SIGUE al foco.
///
/// El defecto: bajar con el mando más allá de la última fila visible movía una
/// selección que no se veía. La lista se quedaba quieta, el jugador apretaba
/// abajo y no pasaba nada en pantalla — que a tres metros se lee como que la
/// aplicación se colgó. Y es el peor lugar para que pase: en una carpeta de
/// sistema, las primeras diez entradas nunca son la que se busca.
///
/// Este test mira la forma y no el píxel: que el `ListView` del explorador
/// reaccione al foco y escriba `viewport-y`. Lo que no puede volver es que esa
/// reacción no exista.
#[test]
fn the_folder_explorer_follows_focus() {
    // #678 — la lista vive en `ListaExplorador`, que ahora comparten el wizard
    // y el diálogo de packs: UNA implementación del seguimiento para los dos.
    let ui = ui();
    let start = ui
        .find("component ExplorerList")
        .expect("no está el componente compartido");
    let start = ui[start..]
        .find("list-view := ListView {")
        .expect("el explorador ya no tiene su lista")
        + start;
    let end = ui[start..]
        .find("for row[fi] in root.rows")
        .expect("la lista no recorre las filas")
        + start;
    let list_header = &ui[start..end];

    assert!(
        list_header.contains("changed focus-index =>"),
        "la lista del explorador no reacciona al foco: el cursor se va a mover fuera de la vista"
    );
    assert!(
        list_header.matches("self.viewport-y =").count() >= 2,
        "tiene que traer la fila a la vista por ARRIBA y por ABAJO"
    );
    // Y el desplazamiento se hace a mano, no con un binding: un binding a
    // `viewport-y` pisa la rueda del mouse, y este diálogo también se usa con
    // mouse.
    assert!(
        !list_header.contains("viewport-y:"),
        "`viewport-y` está bindeado: eso deja la rueda del mouse muerta"
    );
}

/// Ningún modal se queda sin forma de salir con el mando.
///
/// El defecto original: cinco superficies ignoraban el mando, así que abrir la
/// encuesta o el conflicto de guardado con el mouse dejaba al jugador
/// encerrado — la definición de trampa de foco.
#[test]
fn no_modal_is_a_focus_trap() {
    let rs = router();
    // En el `match` de `enrutar`, cada brazo de superficie modal tiene que
    // atender `Back` con algo que no sea `Nada`.
    for (surface, output) in [
        ("Survey =>", "SurveyLater"),
        ("InstallPack =>", "InstallClose"),
        ("Credits =>", "CloseCredits"),
        ("GameDetails =>", "CloseDetails"),
        ("PreLaunch =>", "CancelConfig"),
        // Desde Ajustes, arriba está la Biblioteca (1) — no Inicio, que es la
        // raíz. Cada superficie sube UN nivel, no salta al fondo.
        ("Settings =>", "GoToTab(1)"),
        // #588 — el wizard y su explorador: B retrocede, y nunca cierra Play.
        ("Onboarding =>", "OnboardingBack"),
        ("Explorer =>", "OnboardingBack"),
    ] {
        let i = rs
            .find(surface)
            .unwrap_or_else(|| panic!("falta el brazo {surface}"));
        let match_arm = body(&rs, i);
        assert!(
            match_arm.contains(output),
            "la superficie {surface} no ofrece salida con Atrás (se esperaba {output})"
        );
    }
}

/// #592 — la grilla no se navega como una lista.
///
/// Estas reglas viven en el `.slint` y no hay forma de ejercitarlas sin abrir una
/// ventana, así que se comprueban leyendo el archivo. Suena poco, pero cubre el
/// modo de fallo real: alguien «simplifica» la navegación a ±1 —que es lo que
/// era antes de la grilla y lo que sigue siendo correcto en la lista— y en una
/// grilla de seis columnas subir pasa a mover al vecino de al lado. No falla, no
/// avisa: se siente roto y nadie sabe por qué.
#[test]
fn the_grid_jumps_by_row_and_walks_the_row_with_left_right() {
    let ui = ui();

    // Arriba/abajo: un salto de COLUMNAS, y sólo en grilla.
    assert!(
        ui.contains("current-game -= columns;"),
        "arriba en grilla tiene que saltar una fila entera"
    );
    assert!(
        ui.contains("current-game += columns;"),
        "abajo en grilla tiene que saltar una fila entera"
    );
    assert!(
        ui.contains("if list-view {"),
        "el paso lo decide la VISTA: en lista sigue siendo de a uno"
    );

    // #629 — izquierda/derecha recorren la biblioteca ENTERA y NUNCA cambian de
    // sección: de sección se cambia con LB/RB. La regla vieja —quedarse en la
    // fila y saltar de sección en el borde— hacía de la última tarjeta de cada
    // fila un callejón, y mandaba al Hub a quien insistiera hacia la derecha.
    let start = ui
        .find("if event.text == Key.LeftArrow {\n                    if current-game > 0")
        .expect("izquierda en la biblioteca ya no es un ±1 sobre toda la lista");
    let arrow_handlers = &ui[start..start + 400];
    assert!(
        arrow_handlers.contains("current-game -= 1;")
            && arrow_handlers.contains("current-game += 1;"),
        "←→ tienen que moverse de a un juego, que es lo que hace el wrap de fila"
    );
    assert!(
        !arrow_handlers.contains("active-tab"),
        "←→ en la biblioteca no pueden cambiar de sección: para eso están LB y RB"
    );
    assert!(
        arrow_handlers.contains("current-game + 1 < games.length"),
        "la derecha del último juego no hace nada: no hay a dónde ir"
    );

    // Las columnas salen del ancho real, no de una constante: con un número fijo
    // una ventana angosta recorta la última columna. Y #605 descuenta la safe
    // area (96 px por lado), porque la grilla vive DENTRO de ella: sin
    // descontarla la cuenta daba una columna de más y la última quedaba a
    // medias contra el borde — justo lo que el margen de televisor evita.
    assert!(
        ui.contains("floor((library-width - 192px + CardMetrics.gap)")
            && ui.contains("/ (CardMetrics.width + CardMetrics.gap))"),
        "las columnas se derivan del ancho útil (la tarjeta y su gap, menos los 96 de \
         safe area por lado) y de `CardMetrics`, que es con lo que se DIBUJAN: con los \
         números copiados, cambiar el ancho de la tarjeta deja la grilla contando otra cosa"
    );
}

/// #629 — LA GRILLA SIGUE AL FOCO POR ASIGNACIÓN, NO POR BINDING.
///
/// El defecto: `viewport-y: -contenedor.deseado` era un binding, y un Flickable
/// ASIGNA su `viewport-y` en cuanto el puntero lo toca — un clic cuenta, aunque
/// no arrastre un píxel. En Slint una asignación descarta el binding para
/// siempre, así que después del primer clic en cualquier tarjeta la grilla
/// dejaba de seguir al foco por el resto de la sesión: bajar until la última
/// fila movía una selección que no se veía.
///
/// No falla, no avisa, y no aparece probando con el teclado desde el arranque
/// —ahí el binding todavía vive—: hace falta haber tocado la grilla con el
/// mouse. Es el mismo patrón (y el mismo motivo) que el `ListView` del
/// explorador de carpetas.
///
/// El `changed` va sobre `fila-foco` y no sobre `deseado`: un `changed x` corre
/// mientras Slint evalúa `x`, y leer `deseado` dentro de su propio `changed`
/// re-entra en la propiedad que se está calculando — la ventana muere con
/// «Recursion detected» al primer movimiento del cursor. Por la misma razón no
/// hay `init` que lo aplique de entrada.
#[test]
fn the_grid_follows_focus_even_after_a_click() {
    let ui = ui();
    let start = ui
        .find("if !root.list-view: grid-container := Rectangle {")
        .expect("no está el contenedor de la grilla");
    let end = ui[start..]
        .find("if root.list-view: ListView")
        .map(|i| start + i)
        .unwrap_or(ui.len());
    let grid_section = &ui[start..end];

    assert!(
        grid_section
            .contains("changed row-focus => { game-grid-scroll.viewport-y = -self.desired; }"),
        "la grilla tiene que ASIGNAR su desplazamiento cuando cambia la fila del foco: \
             con un binding, el primer clic sobre una tarjeta lo descarta y el cursor \
             se va de la vista sin que nada lo diga"
    );
    assert!(
        !grid_section.contains("viewport-y: -"),
        "volvió el binding de `viewport-y`: el Flickable lo descarta al primer clic"
    );
    assert!(
        !grid_section.contains("init => { self.viewport-y"),
        "un `init` que lee `deseado` entra en el layout a medio construir: \
             «Recursion detected» antes de dibujar"
    );
}

/// #639 — INICIO SIGUE AL FOCO POR ASIGNACIÓN, NO POR BINDING.
///
/// El mismo defecto que #629 encontró en la grilla, en la otra superficie que
/// se desplaza: `viewport-y: -Math.min(scroll, tope)` era un binding, y un
/// Flickable ASIGNA su `viewport-y` en cuanto el puntero lo toca — un clic
/// cuenta, aunque no arrastre un píxel. En Slint una asignación descarta el
/// binding para siempre, así que después del primer clic en la sección las
/// flechas movían un scroll que ya no llegaba a la vista: «Novedades» quedaba
/// otra vez inalcanzable sin mouse, que es exactamente lo que ese scroll vino
/// a evitar.
///
/// No falla, no avisa, y no aparece probando con el teclado desde el arranque
/// —ahí el binding todavía vive—: hace falta haber tocado la sección con el
/// mouse.
///
/// El `changed` va sobre `scroll` y no sobre `deseado`: un `changed x` corre
/// mientras Slint evalúa `x`, y leer `deseado` dentro de su propio `changed`
/// re-entra en la propiedad que se está calculando — «Recursion detected» al
/// primer movimiento del cursor. Por la misma razón no hay `init`, que además
/// leería `tope` con el layout a medio construir.
#[test]
fn home_follows_focus_even_after_a_click() {
    let ui = ui();
    let start = ui
        .find("home-scope := FocusScope")
        .expect("no está la sección Home");
    let end = ui[start..]
        .find("// ---- #599: Settings")
        .map(|i| start + i)
        .unwrap_or(ui.len());
    let home_section = &ui[start..end];

    assert!(
        home_section.contains("changed scroll => { home-scroll.viewport-y = -self.desired; }"),
        "Home tiene que ASIGNAR su desplazamiento cuando cambia `scroll`:              con un binding, el primer clic en la sección lo descarta y las flechas              mueven una vista que ya no se mueve"
    );
    assert!(
        !home_section.contains("viewport-y: -"),
        "volvió el binding de `viewport-y`: el Flickable lo descarta al primer clic"
    );
    assert!(
        !home_section.contains("init => { self.viewport-y"),
        "un `init` que lee `deseado` entra en el layout a medio construir:              «Recursion detected» antes de dibujar"
    );
}

/// #592 — el semáforo distingue por ÍCONO + TEXTO, nunca sólo por color (§10).
///
/// Es el CA de accesibilidad de la issue, y es exactamente lo que se pierde al
/// «limpiar» un badge dejándole nada más el color de fondo.
#[test]
fn the_traffic_light_says_its_state_in_words() {
    let ui = ui();
    // #629 — la tabla vive en el global `Semaforo` y no adentro del badge: la
    // pintan DOS superficies —el badge de la lista y de la ficha, y la banda de
    // la tarjeta— y con dos copias la grilla puede terminar diciendo
    // «Compatible» donde la ficha dice «Con advertencias» del mismo pack.
    let start = ui
        .find("global StatusLight")
        .expect("no existe el global StatusLight");
    let end = ui[start..]
        .find("\ncomponent ")
        .map(|i| start + i)
        .unwrap_or(ui.len());
    let status_light = &ui[start..end];

    for label in [
        "Compatible",
        "Incompatible",
        "Con advertencias",
        "No verificado",
        "Falta el core",
    ] {
        assert!(
            status_light.contains(label),
            "el estado «{label}» no tiene texto: quedaría distinguido sólo por color"
        );
    }
    // #627 — y el ícono es el de Phosphor que nombra el diseño, no el carácter
    // más parecido que hubiera en las fuentes de texto (`✓`, `×`, `!`, `?`).
    for icon in [
        "Icons.check-circle",
        "Icons.warning",
        "Icons.x-circle",
        "Icons.question",
    ] {
        assert!(
            status_light.contains(icon),
            "falta el ícono `{icon}` en el semáforo"
        );
    }
    // Y las dos superficies preguntan por los tres canales a ESA tabla.
    for f in [
        "StatusLight.icon(",
        "StatusLight.word(",
        "StatusLight.tone(",
    ] {
        assert!(
            ui.matches(f).count() >= 2,
            "`{f}` se usa en menos de dos superficies: alguna se hizo su propia tabla"
        );
    }
}

/// #629 — LOS TILES DE UNA FILA MIDEN LO MISMO Y ESCRIBEN IGUAL.
///
/// El defecto: el alto salía del contenido y el cuerpo del valor bajaba a 22 px
/// cuando el texto pasaba de diez caracteres, así que en la ficha convivían
/// «recién empezado» —chico, en una caja más baja— con «Nunca» y «Mega Drive»
/// en 32. Tres cajas distintas para tres datos del mismo rango se leen como
/// tres cosas de distinta importancia, y no lo son.
///
/// Lo que cede es el ANCHO, que crece con el valor: encoger la letra rompe la
/// fila y elidir muestra un dato cortado, que se lee como un error de la
/// aplicación.
#[test]
fn the_stat_tiles_share_height_and_body() {
    let ui = ui();
    let start = ui
        .find("component StatTile")
        .expect("no existe el StatTile");
    let end = ui[start..]
        .find("\ncomponent ")
        .map(|i| start + i)
        .unwrap_or(ui.len());
    let tile = &ui[start..end];

    assert!(
        !tile.contains("character-count"),
        "el cuerpo del valor volvió a depender del largo del texto: dos tiles \
             hermanos con dos tamaños distintos"
    );
    assert_eq!(
        tile.matches("font-size:").count(),
        2,
        "el tile tiene que declarar UN tamaño para la etiqueta y UNO para el valor"
    );
    assert!(
        tile.contains("height: root.tile-height;")
            && tile.contains("out property <length> tile-height:"),
        "el alto del tile tiene que ser una constante: derivarlo del contenido lo \
             hace depender de que ninguna etiqueta traducida ocupe dos renglones"
    );
    assert!(
        tile.contains("width: max(260px,"),
        "los 260 del .pen son el MÍNIMO: el ancho cede antes que el cuerpo del número"
    );
}

/// #629 — LA FILA DE ACCIONES DE LA FICHA COMPARTE LÍNEA.
///
/// Mezcla alturas —el primario mide 56, los otros botones 48 y el conmutador de
/// HD 32— y un `HorizontalLayout` no centra lo que no se estira: sin envolver,
/// cada uno cuelga del borde de arriba y «Configuración» y «Guardados» se leen
/// unos píxeles por encima de «Jugar».
#[test]
fn the_card_actions_are_vertically_centered() {
    let ui = ui();
    let start = ui
        .find("// actions del .pen: el primario con su")
        .expect("no está la fila de acciones de la ficha");
    // La fila termina donde empiezan los tiles del juego, que es lo que va
    // debajo. (Antes se delimitaba con la explicación de por qué no se puede
    // jugar, que #632 retiró: esa razón vive ahora en la sección ESTADO.)
    let end = ui[start..]
        .find("StatTile {")
        .map(|i| start + i)
        .expect("la fila de acciones ya no está seguida por los tiles del juego");
    let row = &ui[start..end];

    // Cada control de la fila abre INMEDIATAMENTE después de un `EnMedio`: el
    // primario, el conmutador y los dos secundarios (y «Partida nueva», que
    // aparece cuando hay guardado).
    //
    // Se mira el vecino de cada control y no la CUENTA de envoltorios: la fila
    // también envuelve cosas que no son controles —el motivo de por qué el pack
    // no se puede activar (#631)— y una cuenta se rompe con cada una de ésas sin
    // que nada esté mal.
    let mut control_count = 0;
    for (i, _) in row
        .match_indices("ActionButton {")
        .chain(row.match_indices("ToggleHD {"))
    {
        control_count += 1;
        let before: String = row[..i]
            .rsplit('\n')
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .take(1)
            .collect();
        assert!(
            before.ends_with("Centered {"),
            "un control de la fila de acciones no está envuelto en `Centered` \
                 (la línea de arriba dice «{before}»): va a colgar del borde de arriba"
        );
    }
    assert!(
        control_count >= 4,
        "la fila de acciones perdió controles: sólo quedan {control_count}"
    );
}

/// HAY UN SOLO CAMINO DE INSTALACIÓN, Y AUDITA ANTES DE TOCAR LO INSTALADO.
///
/// En AYTHER Play este oráculo cuenta los llamadores del ciclo del Hub —ficha,
/// manifiesto, enlace, descarga reanudable, digest, sello, promoción atómica—
/// para que no se copie a una segunda superficie. En CE ese ciclo no existe:
/// un pack entra por `install_local`, desde el disco, y ése es el único camino.
///
/// Lo que hay que proteger es otra cosa, y es la que hace que CE sea usable sin
/// un tercero de confianza: **la auditoría corre sobre el staging y ANTES de
/// mover nada**. Si alguien la corriera después de promover —o la salteara
/// porque «el motor lo vuelve a mirar al abrir»—, un `.ay` truncado se llevaría
/// puesto el pack que venía funcionando, y el jugador se enteraría al querer
/// jugar. Acá no hay una copia en el servidor para volver a bajar.
///
/// Esto mira el código y no la pantalla, que es donde vive el riesgo.
#[test]
fn installing_a_pack_audits_before_touching_what_is_installed() {
    let rs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/pack_install.rs"),
    )
    .unwrap();

    // NADA que hable con un servicio: no hay sello, ni descarga, ni digest
    // contra lo que alguien publicó (ADR-005).
    for forbidden in ["promote_con_sello", "descargar(", "hub", "reqwest"] {
        assert!(
            !rs.contains(forbidden),
            "`pack_install` volvió a nombrar `{forbidden}`: en CE el pack entra                  desde el disco y no hay servicio del otro lado"
        );
    }

    let start = rs.find("pub fn promote(").expect("no está `promote`");
    let body = &rs[start..];
    let audit = body
        .find("verifier::audit(&staging)")
        .expect("`promote` dejó de auditar: es lo ÚNICO que CE puede comprobar de un pack");
    let move_by = body
        .find("std::fs::rename(&current, &previous)")
        .expect("`promote` dejó de conservar la versión anterior");
    assert!(
        audit < move_by,
        "la auditoría corre DESPUÉS de mover lo instalado: un .ay roto se llevaría              puesto el pack que funcionaba, y acá no hay copia en un servidor para              volver a bajarla"
    );

    // Y que el MOTOR pueda abrirlo, que es otra pregunta: un manifest
    // incompleto pasa la integridad y después no abre.
    assert!(
        body.contains("AyArchive::open_verbose"),
        "`promote` dejó de comprobar que el motor pueda abrir el pack"
    );
}

/// #633 — CADA ESTADO DEL PACK TIENE SUS PALABRAS, Y UNA SOLA ACCIÓN.
///
/// La máquina —qué estado hay y qué acción ofrece— vive en `game_details::EstadoPack`
/// y se prueba sin ventana. Lo que este oráculo cubre es la otra mitad: que la
/// UI tenga una etiqueta por estado y un verbo por acción, y que el conmutador
/// de HD no vuelva.
///
/// El conmutador se fue porque contaba otra historia: escribía el perfil de la
/// próxima sesión y había que deducir del color del riel si el pack estaba
/// puesto. Lo que se elige no es «original o mejorado» dentro de un pack, es si
/// el pack está puesto — y eso tiene nombre.
#[test]
fn the_pack_goes_on_and_off_with_a_verb() {
    let ui = ui();

    assert!(
        !ui.contains("ToggleHD"),
        "volvió el conmutador de HD: la acción sobre el pack vive en su sección"
    );

    // Una etiqueta por estado (activo · apagado · no se puede usar) y un verbo
    // por acción. El del estado «sin pack» es el de CE: instala desde el disco
    // (#678) — el «Conseguir un pack» de AYTHER Play lleva al Hub.
    for word in [
        "Activo",
        "Apagado",
        "No se puede usar",
        "Instalar un pack desde el disco",
        "Desactivar",
        "Activar",
    ] {
        assert!(
            ui.contains(&format!("@tr(\"{word}\")")),
            "la UI no dice «{word}»: un estado sin nombre o un botón sin verbo"
        );
    }

    // El estado de la sección sale del índice que publica `EstadoPack`, no de
    // una segunda derivación en la UI.
    assert!(
        ui.contains("details-pack-usage-status == 1 ? @tr(\"Activo\")"),
        "las palabras del estado tienen que salir del índice de `PackState`"
    );

    // Y X del mando dice lo mismo que el botón.
    assert!(
        ui.contains("@tr(\"Activar/Desactivar pack\")"),
        "el pie sigue anunciando «Original / HD»: nombra un perfil, no lo que se elige"
    );

    // Y el botón del estado sin pack abre el INSTALADOR, no otra sección: el
    // callback de `ficha-pack-accionar` en el 0 tiene que terminar en el
    // diálogo de #678 y no en un `set_active_tab`.
    let rs_main =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
            .unwrap();
    let i = rs_main
        .find("on_details_pack_act")
        .expect("no está el callback del pack");
    let match_arm = &rs_main[i..i + 1600];
    assert!(
        match_arm.contains("install::Mode::File"),
        "el estado sin pack tiene que abrir el diálogo de instalar (#678)"
    );
    assert!(
        !match_arm.contains("set_active_tab"),
        "el botón del pack no puede mandar a otra sección: eso hacía el de Play"
    );

    let rs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/game_details.rs"),
    )
    .unwrap();
    assert!(
        rs.contains("pub enum PackState") && rs.contains("pub fn action(self)"),
        "la tabla estado→acción tiene que seguir en Rust, donde se puede probar"
    );
}

/// #632 — LA FICHA TIENE UN SEMÁFORO Y UNA SECCIÓN, NO TRES EJES HERMANOS.
///
/// Los tres badges eran dos preguntas disfrazadas de tres: «¿corre?» —que la
/// contesta el core y decide si se juega— y «¿está mejorado?» —que la contesta
/// el pack y es opcional—. El eje ROM nunca fue un tercero: `eje_rom` sólo lee
/// códigos del validador DEL PACK y sin pack no dice nada, así que al lado del
/// core se leía como «tu ROM está mal» y mandaba a arreglar lo que no está roto.
///
/// Lo que esto fija es que la razón de la ROM **no se perdió** en la mudanza:
/// sigue en la ficha, adentro de la sección del pack, que es de lo que habla.
#[test]
fn the_card_does_not_go_back_to_three_axes() {
    let ui = ui();

    assert!(
        !ui.contains("AxisRow"),
        "volvieron las filas de eje: la ficha contesta dos preguntas, no tres"
    );
    // Los USOS, no las menciones: los comentarios de arriba nombran los ejes
    // viejos para explicar por qué se fueron, y eso tiene que poder escribirse.
    for axis in ["root.eje-rom", "root.eje-core", "root.eje-pack"] {
        assert!(
            !ui.contains(axis),
            "volvió el uso de `{axis}`: eran dos preguntas, no tres"
        );
    }

    // Un semáforo de ESTADO, alimentado por el core.
    let i = ui
        .find("text: @tr(\"ESTADO\");")
        .expect("la ficha no tiene sección de estado");
    let section = &ui[i..i + 900];
    assert!(
        section.contains("state: root.details-status;"),
        "el semáforo de estado tiene que salir del eje del core"
    );
    assert!(
        section.contains("root.details-status-reason"),
        "y decir por qué, al lado: un badge sin razón manda a probar de nuevo"
    );

    // Y una sección de PACK que se queda con el motivo largo — el lugar donde
    // aterrizó la frase del eje ROM.
    let i = ui
        .find("text: @tr(\"PACK\");")
        .expect("la ficha no tiene sección de pack");
    let section = &ui[i..];
    assert!(
        section.contains("root.details-pack-reason"),
        "el motivo del pack (que incluye el de la ROM) tiene que estar en su sección"
    );
    let rs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/game_details.rs"),
    )
    .unwrap();
    assert!(
        rs.contains("pub fn pack_reason"),
        "`pack_reason` es lo que hace que la razón de la ROM sobreviva a la mudanza"
    );
}

/// #629 — EL SEMÁFORO DE LA TARJETA VA SOBRE LA CARÁTULA Y SÓLO CON EL FOCO.
///
/// Era una fila más de la tarjeta: 36 px que TODAS pagaban para contestar una
/// pregunta que sólo se hace sobre el juego que uno está mirando, y seis
/// «Compatible» verdes por fila que el ojo aprende a no ver. Un semáforo
/// encendido siempre no informa, decora.
///
/// La banda es un velo NEGRO y no un color de la paleta porque lo que tiene
/// debajo es una carátula cualquiera: sobre un cielo claro, `bg-elevated` deja
/// el texto ilegible. Por eso no es —ni debe ser— un token del `.pen`.
#[test]
fn the_tile_traffic_light_is_a_band_over_the_cover_with_focus() {
    let ui = ui();

    let start = ui
        .find("component StatusBand")
        .expect("no existe la StatusBand");
    let end = ui[start..]
        .find("\ncomponent ")
        .map(|i| start + i)
        .unwrap_or(ui.len());
    let status_band = &ui[start..end];
    assert!(
        status_band.contains("Colors.black.with-alpha(0.75)"),
        "la banda tiene que ser el velo negro al 75%: sobre una carátula clara, un \
             fondo de la paleta deja el texto ilegible"
    );

    // Y en la tarjeta: sólo con el foco, y apoyada en el borde inferior de la
    // carátula.
    let start = ui
        .find("component GameCard")
        .expect("no existe la GameCard");
    let end = ui[start..]
        .find("\ncomponent ")
        .map(|i| start + i)
        .unwrap_or(ui.len());
    let card = &ui[start..end];
    assert!(
        card.contains("if root.focused: StatusBand"),
        "la banda de la tarjeta tiene que aparecer sólo con el foco"
    );
    assert!(
        card.contains("y: parent.height - self.height;"),
        "la banda va al pie de la carátula, no flotando en el medio"
    );
    assert!(
        !card.contains("SemaphoreBadge"),
        "la tarjeta no puede volver a tener el badge como fila propia: el alto de \
             `CardMetrics` ya no lo contempla"
    );
}

// La lista negra de glifos que había acá se retiró: sólo cubría los codepoints
// que a alguien se le habían ocurrido, y dejó pasar los seis del menú de #593.
// La reemplaza `tests/glyphs.rs`, que lee la `cmap` de las fuentes vendorizadas
// y pregunta por los que el `.slint` usa de verdad.

/// NADIE PIDE EL FOCO EN `init`. Es el crash que cerraba Play «de la nada».
///
/// Pedirlo mientras Slint construye el subárbol reentra en la ventana: el
/// adaptador de winit avisa del cambio de foco y hace `borrow_mut()` sobre el
/// RefCell de AccessKit que esa misma construcción ya tiene prestado
/// (`winitwindowadapter.rs:1584`; la función de al lado, `register_item_tree`,
/// usa `try_borrow_mut` y comenta exactamente este caso). La ventana muere con
/// «RefCell already borrowed».
///
/// No es determinista —depende de qué esté haciendo AccessKit ese frame— así
/// que no aparece al entrar una vez en una sección, sino después de veinte
/// cambios seguidos con LB/RB. Por eso hace falta un oráculo: probando a mano
/// se pasa, y el que lo re-introduzca lo va a descubrir un jugador.
///
/// La forma correcta es `FocoDiferido`, que lo pide un tick después.
#[test]
fn no_screen_takes_focus_while_it_is_being_built() {
    let ui = ui();
    let offenders: Vec<(usize, &str)> = ui
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            let l = l.trim_start();
            // `init => { … .focus() … }` en cualquiera de sus formas.
            l.starts_with("init =>") && l.contains(".focus()")
        })
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();

    assert!(
        offenders.is_empty(),
        "estas líneas piden el foco durante la construcción y pueden cerrar Play \
         con «RefCell already borrowed» — usá `DeferredFocus`:\n{}",
        offenders
            .iter()
            .map(|(n, l)| format!("  appwindow.slint:{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Y el componente que lo hace bien tiene que existir: sin él, el arreglo de
    // arriba se «cumple» borrando el foco de todas las pantallas, que deja el
    // mando sin superficie y es peor que el crash.
    assert!(
        ui.contains("component DeferredFocus"),
        "falta `DeferredFocus`: sin él nadie toma el foco y el mando queda mudo"
    );
}

/// UN MODAL COME LOS CLICS DE LO QUE TAPA.
///
/// El velo oscuro sin `TouchArea` es decoración: el mando no llega al fondo
/// —el ruteo lo impide— pero el mouse sí. Se podía cambiar de sección con la
/// diálogo abierto, que es cómo alguien lo deja pendiente para siempre sin
/// darse cuenta de que lo contestó.
#[test]
fn modals_do_not_let_you_touch_what_is_behind() {
    let ui = ui();
    // Comer los clics lo garantiza `Velo` —su TouchArea vive en el componente—
    // así que lo que se verifica es que los diálogos lo USEN, y no un
    // Rectangle propio que se olvide del TouchArea.
    for modal in [
        "if confirm-open: Scrim {",
        "if saves-open: Scrim {",
        "if install-open: Scrim {",
        "if crash-open: Scrim {",
        "if orphan-open: Scrim {",
        "if feedback-open: Scrim {",
    ] {
        assert!(ui.contains(modal), "el diálogo de «{modal}» no usa `Scrim`");
    }
    let scrim = ui
        .find("component Scrim inherits Rectangle {")
        .expect("no está `Scrim`");
    assert!(
        ui[scrim..scrim + 900].contains("TouchArea"),
        "`Scrim` perdió su TouchArea: los diálogos dejan tocar lo de atrás"
    );
}

/// EL VELO DE UN DIÁLOGO CUBRE EL CUERPO, NO EL CHASIS — regla general.
///
/// Las dos barras siguen valiendo con el diálogo abierto: la TopBar dice dónde
/// estás, y el pie es LA BOTONERA del diálogo — sus hints son los botones para
/// quien usa mouse, así que taparlo deja al diálogo sin botones. El defecto que
/// esto cierra es literal: los velos medían `parent.height` entero y el pie
/// quedaba debajo de un vidrio oscuro que además se comía sus clicks.
#[test]
fn the_veils_cover_the_body_and_not_the_chassis() {
    let ui = ui();

    // Ningún velo crudo a nivel ventana: el único `modal-dim` de primer nivel
    // es el del propio `Velo`. Los anidados (créditos e informe dentro de la
    // pantalla previa, el explorador dentro del wizard) viven DENTRO de un área
    // que ya es el cuerpo, y el pie de su pantalla se dibuja después.
    for line in ui.lines() {
        assert!(
            !line.starts_with("        background: Tokens.modal-dim")
                || line.contains("with-alpha"),
            "hay un velo crudo a nivel ventana: «{}». Usá `Scrim` con la \
             geometría del cuerpo (y: Shell.topbar, alto menos las dos barras)",
            line.trim()
        );
    }

    // Y cada uso de `Velo` declara la geometría del cuerpo. La pone el que lo
    // usa —`parent` no existe dentro de un componente— y por eso se verifica:
    // un `Velo` sin geometría cubre la ventana entera en silencio.
    let mut scrim_count = 0;
    for (i, _) in ui.match_indices(": Scrim {") {
        scrim_count += 1;
        let remainder = &ui[i..];
        let block_end = remainder
            .char_indices()
            .nth(400)
            .map_or(remainder.len(), |(index, _)| index);
        let block = &remainder[..block_end];
        assert!(
            block.contains("y: Shell.topbar;")
                && block.contains("height: parent.height - Shell.topbar - Shell.footer;"),
            "un `Scrim` no declara la geometría del cuerpo:\n{block}"
        );
    }
    assert!(
        scrim_count >= 8,
        "sólo {scrim_count} usos de `Scrim`: el lector se rompió"
    );

    // Los paneles hermanos centran dentro del CUERPO: centrado contra la
    // ventana, un diálogo alto queda metido debajo del pie que lo opera. Son
    // los siete paneles de los diálogos de primer nivel (menú, editar,
    // confirmación, guardados, instalar, crash, huérfana).
    let centered_panel_count = ui
        .matches(
            "y: Shell.topbar + (parent.height - Shell.topbar - Shell.footer - self.height) / 2;",
        )
        .count();
    assert!(
        centered_panel_count >= 7,
        "sólo {centered_panel_count} paneles centran en el cuerpo: alguno volvió a centrarse          contra la ventana y queda metido debajo del pie que lo opera"
    );
}

/// SIN MANDO, EL PIE MUESTRA EL TECLADO — y la tecla que muestra es verdad.
///
/// La detección la hace el hilo de gilrs y la escribe en el global `Mando`;
/// las píldoras consultan `Mando.glifo`, que traduce cada botón a la tecla que
/// los `FocusScope` ya manejan. Y donde A y Start hacen cosas DISTINTAS —la
/// pantalla previa: A cambia, Start juega— el hint de A declara su `kb`
/// («Espacio»), porque con las dos cayendo a Enter el pie prometería jugar con
/// la tecla que cambia una opción.
#[test]
fn without_a_gamepad_the_footer_shows_the_keyboard() {
    let ui = ui();

    // La barra espaciadora es A y Retroceso es B: las dos teclas grandes para
    // las dos acciones de siempre. Enter queda para Start — y con eso A y
    // Start no comparten tecla en ninguna pantalla.
    let g = ui
        .find("pure public function glyph")
        .expect("no está `Gamepad.glifo`");
    let glyph = &ui[g..g + 800];
    for (pad, key) in [
        ("\"A\"", "Espacio"),
        ("\"B\"", "Retroceso"),
        ("\"LB\"", "Re Pág"),
        ("\"RB\"", "Av Pág"),
        ("\"Start\"", "Enter"),
    ] {
        assert!(
            glyph.contains(pad) && glyph.contains(key),
            "`Gamepad.glifo` no traduce {pad} a «{key}»"
        );
    }

    // Y las teclas dibujadas EXISTEN: donde un FocusScope maneja Escape como
    // «atrás», maneja también Retroceso. La única Escape suelta permitida es
    // la del campo de búsqueda (usa `e.text`, no `event.text`): ahí Retroceso
    // BORRA texto, que es su otro trabajo.
    // La excepción de la CAPTURA se declara en la propia línea («capturando:»):
    // ahí Retroceso es una tecla ASIGNABLE, así que emparejarla con Escape la
    // volvería inasignable — la única salida de una captura es Esc.
    for line in ui.lines() {
        if line.contains("event.text == Key.Escape") && !line.contains("// capturando:") {
            assert!(
                line.contains("Key.Backspace"),
                "una Escape sin su Retroceso: «{}» — el pie dibuja Retroceso \
                     para B, así que la tecla tiene que responder",
                line.trim()
            );
        }
    }

    // La píldora consulta al global: si dibujara `tecla` a secas, el pie
    // seguiría mostrando botones de un mando que no está.
    let pg = ui.find("component PadGlyph").expect("no está PadGlyph");
    assert!(
        ui[pg..pg + 700].contains("Gamepad.glyph("),
        "PadGlyph no consulta `Gamepad.glifo`: sin mando dibuja botones de mando"
    );

    // Y toda tecla que un hint dibuja es una que el router entiende: es lo que
    // hace válida la traducción Y el click (gamepad::accion_de_hint).
    for (i, _) in ui.match_indices("{ tecla: \"") {
        let remaining = &ui[i + "{ tecla: \"".len()..];
        let t = &remaining[..remaining.find('\"').unwrap()];
        // Vacía = hint de dirección: lleva ícono y su click no es un botón.
        assert!(
            matches!(t, "" | "A" | "B" | "X" | "Y" | "Start" | "LB" | "RB"),
            "el hint dibuja «{t}», que ni el router ni el teclado conocen"
        );
    }
}

/// EL PIE ES BOTONERA: el click entra por el MISMO router que el mando.
///
/// Por esto los velos no lo tapan. Un camino de click propio es como el mouse
/// y el mando empiezan a hacer cosas distintas con el mismo botón dibujado.
#[test]
fn a_click_on_the_footer_goes_through_the_router() {
    let ui = ui();
    let footer = ui
        .find("hints: root.shell-hints;")
        .expect("no está el pie del chasis");
    let block = &ui[footer.saturating_sub(2000)..footer];
    assert!(
        block.contains("pressed(key) => { root.hint-pressed(key); }"),
        "el pie del chasis no manda sus clicks a `hint-pressed`"
    );

    let rs = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .unwrap();
    let i = rs
        .find("on_hint_pressed")
        .expect("main.rs no maneja `hint-pressed`");
    let match_arm = &rs[i..i + 800];
    assert!(
        match_arm.contains("gamepad::hint_action") && match_arm.contains("route"),
        "el click del pie tiene un camino propio en vez de pasar por el router"
    );
}

// ---- #653: el recorrido de pestañas -------------------------------------

/// Devuelve el cuerpo de un bloque `{ … }` a partir de un índice cualquiera:
/// abre en la primera llave que encuentra y cierra en la que la balancea.
fn body(s: &str, since: usize) -> &str {
    let b = s.as_bytes();
    let opening_brace = since + s[since..].find('{').expect("el bloque no abre llave");
    let mut level = 0usize;
    for i in opening_brace..b.len() {
        match b[i] {
            b'{' => level += 1,
            b'}' => {
                level -= 1;
                if level == 0 {
                    return &s[opening_brace..=i];
                }
            }
            _ => {}
        }
    }
    panic!("el bloque que empieza en {since} no cierra");
}

/// Las secciones del chasis: por cada `FocusScope` guardado por `active-tab`,
/// qué índices cubre y si su manejador declara el paso de sección con teclado.
///
/// Se lee del archivo y no de una lista escrita acá: una séptima pestaña sin
/// `FocusScope` —o una existente a la que alguien le saque las teclas— tiene
/// que hacer fallar esto, y un oráculo con su propia lista no se entera.
fn sections_with_a_walk(ui: &str) -> Vec<(Vec<i32>, bool, bool)> {
    let mut out = Vec::new();
    for (i, _) in ui.match_indices(":= FocusScope {") {
        // La guarda es lo que va desde el `if` de la línea until el `:`.
        let line_start = ui[..i].rfind('\n').map_or(0, |n| n + 1);
        let guard = &ui[line_start..i];
        if !guard.trim_start().starts_with("if ") || !guard.contains("active-tab ==") {
            continue;
        }
        let tabs: Vec<i32> = guard
            .match_indices("active-tab ==")
            .filter_map(|(p, _)| {
                guard[p + "active-tab ==".len()..]
                    .trim_start()
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .filter(|d| !d.is_empty())
                    .and_then(|d| d.parse().ok())
            })
            .collect();
        // Sólo el PRIMER `key-pressed` del bloque, que es el de la sección:
        // mirar el bloque entero dejaría que un manejador anidado —el de un
        // modal, por ejemplo— contestara por ella.
        let block = body(ui, i);
        let (previous, next) = match block.find("key-pressed(event) =>") {
            Some(k) => {
                let h = body(block, k);
                (
                    h.contains("Key.PageUp") && h.contains("go-to-section(-1)"),
                    h.contains("Key.PageDown") && h.contains("go-to-section(1)"),
                )
            }
            None => (false, false),
        };
        out.push((tabs, previous, next));
    }
    out
}

/// De cualquier sección se llega a cualquier otra sin tocar el mouse.
///
/// EL DEFECTO (#653): en una aplicación que se define por ser operable con
/// mando y a distancia de TV, **Ajustes era un sumidero**. Cada sección
/// declaraba sus transiciones por su cuenta con las flechas, pero las flechas
/// además recorren el contenido: la biblioteca se las quedaba para su grilla y
/// no le dejaba ninguna al chasis. Ninguna transición apuntaba HACIA 5, así que
/// a la sección donde se configuran las carpetas de ROMs, los cores y el idioma
/// sólo se llegaba clickeando su pestaña — y encima Descargas y Perfil no
/// tenían `FocusScope`, o sea que entrar ahí con el teclado dejaba el foco en
/// ninguna parte y ninguna tecla hacía nada.
///
/// El mando se salvaba de casualidad, porque su recorrido vive en
/// `pad_router::enrutar()` y no en el foco de Slint. Por eso este oráculo mira
/// el `.slint`: es la mitad que no tenía quién la mirara.
#[test]
fn the_tab_walk_covers_the_three_sections_without_a_mouse() {
    let ui = ui();

    // Cuántas pestañas hay, leído de la TopBar y no escrito acá.
    let start = ui
        .find("for tab[i] in [")
        .expect("no está la tira de pestañas");
    let end = ui[start..]
        .find("]: Rectangle {")
        .expect("la tira de pestañas no cierra")
        + start;
    let n = ui[start..end].matches("label:").count() as i32;
    assert_eq!(
        n, 3,
        "la TopBar dejó de tener tres pestañas ({n}): revisá el tope del paso"
    );

    // El paso se acota a la tira y no da la vuelta, como el router.
    let step = body(
        &ui,
        ui.find("go-to-section(step) =>")
            .expect("no está `go-to-section`"),
    );
    assert!(
        step.contains(&format!("Math.max(0, Math.min({}, ", n - 1)),
        "`go-to-section` no acota el paso a las {n} pestañas:\n{step}"
    );

    let sections = sections_with_a_walk(&ui);
    let mut covered_tabs: Vec<i32> = Vec::new();
    for (tabs, previous, next) in &sections {
        assert!(
            *previous && *next,
            "la sección de las pestañas {tabs:?} no declara el paso de sección con teclado.\n\
             Re-Pág y Av-Pág son el LB/RB del teclado y van en LAS TRES: las flechas están\n\
             tomadas por el contenido —la grilla de la biblioteca, las filas de Settings— así\n\
             que sin ellas la sección queda sin salida y sus vecinas sin entrada (#653)."
        );
        covered_tabs.extend(tabs);
    }
    covered_tabs.sort();
    assert_eq!(
        covered_tabs,
        (0..n).collect::<Vec<_>>(),
        "las pestañas con `FocusScope` no son las tres de la TopBar: {covered_tabs:?}.\n\
         Una pestaña sin `FocusScope` no recibe teclas: con el teclado se entra y no se sale."
    );

    // Y el grafo que eso produce es conexo: ±1 acotado sobre 0..n llega a todo.
    for since in 0..n {
        for until in 0..n {
            assert!(
                (until - since).abs() < n,
                "de {since} a {until} no se llega"
            );
        }
    }
}

/// Y el oráculo sabe reconocer el defecto: una sección sin las teclas sale
/// señalada. Sin esto, el test pasaría en verde tanto si mide como si no.
#[test]
fn the_oracle_sees_a_section_without_a_walk() {
    let with_navigation = "if active-tab == 9 && x: s := FocusScope {\n\
               key-pressed(event) => {\n\
               if event.text == Key.PageUp { root.go-to-section(-1); }\n\
               if event.text == Key.PageDown { root.go-to-section(1); }\n\
               }\n}";
    assert_eq!(
        sections_with_a_walk(with_navigation),
        vec![(vec![9], true, true)]
    );

    let without_navigation = "if active-tab == 9 && x: s := FocusScope {\n\
               key-pressed(event) => {\n\
               if event.text == Key.LeftArrow { active-tab = 2; }\n\
               }\n}";
    assert_eq!(
        sections_with_a_walk(without_navigation),
        vec![(vec![9], false, false)],
        "una sección sin Re-Pág/Av-Pág tiene que salir señalada: es #653 en su forma mínima"
    );

    // Dos índices en la misma guarda —Descargas y Perfil comparten pantalla—
    // cuentan como dos pestañas cubiertas, no como una.
    let two_tabs = "if (active-tab == 3 || active-tab == 4) && x: s := FocusScope {\n\
               key-pressed(event) => {\n\
               if event.text == Key.PageUp { root.go-to-section(-1); }\n\
               if event.text == Key.PageDown { root.go-to-section(1); }\n\
               }\n}";
    assert_eq!(
        sections_with_a_walk(two_tabs),
        vec![(vec![3, 4], true, true)]
    );

    // Y un `FocusScope` que no es una sección —un modal— no se cuenta.
    let modal = "if telemetry-open: s := FocusScope {\n key-pressed(event) => { }\n}";
    assert!(
        sections_with_a_walk(modal).is_empty(),
        "un modal no es una sección del chasis: contarlo pediría teclas que ahí no van"
    );
}

/// El teclado y el mando recorren la MISMA tira.
///
/// `pad_router` acota el paso en los extremos (`Inicio` ignora `TabAnterior`,
/// `Ajustes` ignora `TabSiguiente`) y el `.slint` lo acota con `Math.max` y
/// `Math.min`. Son dos escrituras de la misma regla, y esto es lo que impide
/// que una empiece a dar la vuelta mientras la otra frena en el extremo.
#[test]
fn the_keyboard_and_the_gamepad_bound_the_strip_at_the_same_ends() {
    let rs = router();
    for (surface, boundary_behavior) in [
        ("Home => match a {", "Left | PrevTab => F::NoOp"),
        ("Settings => match a {", "Right | NextTab => F::NoOp"),
    ] {
        let i = rs
            .find(surface)
            .unwrap_or_else(|| panic!("no está `{surface}`"));
        let arm = body(&rs, i);
        assert!(
            arm.contains(boundary_behavior),
            "`{surface}` dejó de acotar la tira: el mando empezaría a dar la vuelta\n\
             mientras el teclado sigue frenando en el extremo (`Math.max`/`Math.min`)."
        );
    }
}

/// Las filas de Ajustes se cuentan UNA vez.
///
/// Hay tres lugares que tienen que decir lo mismo: las `SettingRow` que el
/// `.slint` dibuja, el tope de la flecha abajo en su manejador, y
/// `settings::FILAS`, que es de donde el enrutador del mando saca cuántas hay.
///
/// El modo de fallo es siempre el mismo y siempre silencioso: alguien agrega
/// una fila, se dibuja perfecta, y no se puede enfocar — ni con el mando, que
/// se frena en la cuenta vieja, ni con el teclado, que se frena en el tope
/// viejo. La fila existe, se ve, y no se puede usar. El propio `settings.rs` ya
/// lo dice en el comentario de `FILAS`; esto lo verifica.
#[test]
fn the_settings_rows_are_counted_only_once() {
    let ui = ui();
    let rs =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/settings.rs"))
            .unwrap();

    // Lo que el .slint dibuja: la sección de Ajustes y sus `SettingRow`.
    let start = ui
        .find("if active-tab == 2 &&")
        .expect("no está la sección de Settings");
    let section = body(&ui, start);
    let rendered_rows = section.matches("SettingRow {").count() as i32;
    assert!(
        rendered_rows >= 5,
        "sólo {rendered_rows} filas: el lector se rompió"
    );

    // Lo que `settings.rs` declara.
    let i = rs
        .find("pub const ROWS: i32 = ")
        .expect("settings.rs no declara ROWS");
    let remaining = &rs[i + "pub const ROWS: i32 = ".len()..];
    let declared_rows: i32 = remaining[..remaining.find(';').unwrap()]
        .trim()
        .parse()
        .unwrap();

    assert_eq!(
        rendered_rows, declared_rows,
        "el .slint dibuja {rendered_rows} rows de Settings y `settings::ROWS` dice {declared_rows}.
         El gamepad_name saca de ROWS until dónde puede bajar: la diferencia es exactamente el          número de rows que se ven y no se pueden enfocar."
    );

    // Y el tope del teclado, que es la tercera cuenta.
    let limit = section
        .find("if setting-row < ")
        .map(|k| {
            let r = &section[k + "if setting-row < ".len()..];
            r[..r.find(' ').unwrap()].trim().parse::<i32>().unwrap()
        })
        .expect("la sección de Settings no acota la flecha abajo");
    assert_eq!(
        limit,
        declared_rows - 1,
        "la flecha abajo se frena en {limit} y hay {declared_rows} filas: con el teclado no se          llega a las últimas {}.",
        declared_rows - 1 - limit
    );

    // Y cada fila dibujada se enfoca por su índice, sin huecos ni repetidas: dos
    // filas con el mismo `ajuste-fila` se resaltan juntas y la de abajo queda
    // inalcanzable.
    let mut indices: Vec<i32> = section
        .match_indices("focused: root.setting-row == ")
        .map(|(k, pat)| {
            let r = &section[k + pat.len()..];
            r[..r.find(';').unwrap()].trim().parse().unwrap()
        })
        .collect();
    indices.sort();
    assert_eq!(
        indices,
        (0..declared_rows).collect::<Vec<_>>(),
        "los índices de las filas de Settings no son 0..{declared_rows} sin repetir"
    );
}
