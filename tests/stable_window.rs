//! El tamaño de la ventana no depende de la sección que se esté mirando (#652).
//!
//! Slint no toma el tamaño de la ventana de lo que declare `Window`: se lo pide
//! al CONTENIDO (`layout_constraints()` recorre el árbol) y el resultado gana
//! sobre cualquier `preferred-*`. Una sección hecha de medidas fijas le impone
//! entonces su mínimo —o su máximo— a la ventana entera, y cambiar de pestaña
//! la redimensiona.
//!
//! Ya pasó tres veces antes de que esto existiera, siempre igual y siempre
//! descubierto a mano:
//!
//! * «modo local» — tres tarjetas de 460 y un texto de 1100 daban un ancho
//!   máximo finito (1620): entrar en Descargas ENCOGÍA la ventana maximizada.
//! * la ficha del juego — la carátula de 400 y las tarjetas de 260 daban 1448,
//!   con el mismo síntoma.
//! * Ajustes (#652) — once filas de 88 px de alto fijo pedían ~1162 px contra
//!   los 912 del área útil: entrar AGRANDABA la ventana de 1048 a 1119 en un
//!   escritorio de 1032, y salir no la devolvía. Ni creciendo entraba, así que
//!   la última fila y los ButtonHints quedaban fuera de la pantalla.
//!
//! Las dos curas son la misma idea —cortar la propagación—: un `Flickable`
//! para el alto (su `viewport-height` es del contenido y su alto lo pone el
//! padre) y un espaciador flexible para el ancho (devuelve el máximo a
//! infinito).
//!
//! ALCANCE, dicho de frente: esto no simula el layout de Slint, así que no
//! puede probar el invariante entero. Mide la forma que produjo los tres casos
//! —filas de alto fijo apiladas en una sección sin `Flickable`— y falla ahí.
//! Una sección que desborde por otra vía pasaría este test; lo que no puede
//! volver a pasar en silencio es ESTA.
//!
//! Como el resto de los tests de Play, no abre una ventana: lee el `.slint`
//! como texto, que es lo único que corre en el CI de Linux sin display.

use std::path::PathBuf;

fn ui() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/appwindow.slint");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

/// El alto del chasis (#605) menos la TopBar y los ButtonHints: lo que le queda
/// de verdad a una sección antes de empezar a empujar la ventana.
const USABLE_HEIGHT: u32 = 1080 - 96 - 72;
/// La safe area vertical del chasis, arriba y abajo.
const SAFE_AREA: u32 = 54 * 2;

/// Devuelve el cuerpo de un bloque `{ ... }` a partir del índice de su llave.
fn body(s: &str, since: usize) -> &str {
    let bytes = s.as_bytes();
    let opening_brace = since + s[since..].find('{').expect("el bloque no abre llave");
    let mut level = 0usize;
    for i in opening_brace..bytes.len() {
        match bytes[i] {
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

/// El alto de `SettingRow`, leído y no escrito: si el componente cambia de
/// alto, la aritmética de este test tiene que cambiar con él y no seguir
/// midiendo contra un número que ya no existe.
fn setting_row_height(ui: &str) -> u32 {
    let start = ui
        .find("component SettingRow inherits Rectangle {")
        .expect("no encontré el componente SettingRow");
    let c = body(ui, start);
    let h = c.find("height: ").expect("SettingRow no declara height");
    let remaining = &c[h + "height: ".len()..];
    let end = remaining
        .find("px")
        .expect("el height de SettingRow no está en px");
    remaining[..end]
        .trim()
        .parse()
        .expect("el height de SettingRow no es un número")
}

/// Las secciones que apilan filas de alto fijo sin poder desplazarlas.
///
/// Separado del `#[test]` para que la lógica se pueda ejercer contra un
/// fixture: un oráculo que sólo se corre contra el archivo bueno no prueba que
/// sepa reconocer el malo — pasa en verde tanto si mide como si no mide nada.
fn sections_that_push_the_window(ui: &str) -> Vec<String> {
    let row_height = setting_row_height(ui);
    assert!(
        row_height > 0,
        "SettingRow con alto 0: la medición no diría nada"
    );
    let mut offenders = Vec::new();
    for (i, _) in ui.match_indices("if active-tab == ") {
        let block = body(ui, i);
        let rows = block.matches("SettingRow {").count() as u32;
        if rows == 0 {
            continue;
        }
        let tile_height = rows * row_height + SAFE_AREA;
        if tile_height > USABLE_HEIGHT && !block.contains("Flickable {") {
            offenders.push(format!(
                "{rows} filas de {row_height} px (= {tile_height} px con la safe area) contra los                  {USABLE_HEIGHT} px del área útil, sin Flickable"
            ));
        }
    }
    offenders
}

/// Ninguna sección apila más filas de alto fijo de las que entran sin
/// desplazamiento.
#[test]
fn a_section_with_fixed_height_rows_that_do_not_fit_has_a_flickable() {
    let ui = ui();
    let offenders = sections_that_push_the_window(&ui);
    assert!(
        offenders.is_empty(),
        "hay secciones que le imponen su mínimo a la VENTANA: {offenders:?}. Slint le pasa ese          mínimo al Window: entrar en esa sección la agranda y salir no la devuelve. Envolvé el          contenido en un Flickable con `viewport-height: <col>.preferred-height`, como Home,          la grilla, la ficha y Settings — y acordate de que el desplazamiento tiene que SEGUIR AL          FOCO, o las filas de abajo quedan inalcanzables (#639, #652)."
    );

    // No-vacuidad: que el `.slint` de verdad tenga la forma que esto mide. Sin
    // este check, renombrar `SettingRow` dejaría el test en verde para siempre.
    let rows: usize = ui
        .match_indices("if active-tab == ")
        .map(|(i, _)| body(&ui, i).matches("SettingRow {").count())
        .sum();
    assert!(
        rows > 0,
        "el test no encontró ninguna fila `SettingRow` dentro de una sección. O el `.slint`          cambió de forma y esto dejó de medir lo que dice medir, o las filas se renombraron: en          los dos casos hay que arreglar el test, no borrarlo."
    );
}

/// Y que sepa reconocer el defecto: el mismo bloque, sin `Flickable`, tiene que
/// salir señalado. Es #652 reducido a su forma mínima.
#[test]
fn the_oracle_recognizes_a_section_without_a_flickable() {
    let base = "component SettingRow inherits Rectangle { height: 88px; }";
    let rows = "SettingRow { }
"
    .repeat(11);

    let invalid_example = format!(
        "{base}
if active-tab == 5 && x: y := FocusScope {{ {rows} }}"
    );
    assert_eq!(
        sections_that_push_the_window(&invalid_example).len(),
        1,
        "once filas de 88 px sin Flickable son 1076 px contra 912: el oráculo tiene que verlo"
    );

    let valid_example = format!(
        "{base}
if active-tab == 5 && x: y := FocusScope {{ Flickable {{ {rows} }} }}"
    );
    assert!(
        sections_that_push_the_window(&valid_example).is_empty(),
        "con Flickable no empuja la ventana y no hay nada que reportar"
    );

    // Y que no sea un detector de «hay muchas filas»: pocas filas entran.
    let few_rows = format!(
        "{base}
if active-tab == 2 && x: y := FocusScope {{ {} }}",
        "SettingRow { }
"
        .repeat(5)
    );
    assert!(
        sections_that_push_the_window(&few_rows).is_empty(),
        "cinco filas entran de sobra: señalarlas sería un falso positivo"
    );
}

/// La sección que ya mordió conserva su cura.
///
/// No es redundante con el test de arriba: aquélla desbordaba por ANCHO, que se
/// arregla con un espaciador flexible y no con un `Flickable`, así que el
/// conteo de filas no la mira.
///
/// Eran dos. La otra era `ModoLocal` —la pantalla de Hub, Descargas y Perfil—,
/// que en CE no existe (ADR-005); la ficha es la que queda, y es la que tiene
/// las medidas fijas: la carátula de 400, las tarjetas de 260, los botones.
#[test]
fn sections_that_already_fixed_their_width_keep_their_spacer() {
    let ui = ui();
    {
        let (component_name, marker) = ("details-column := VerticalLayout {", "HorizontalLayout {");
        let start = ui
            .find(component_name)
            .unwrap_or_else(|| panic!("no encontré `{component_name}` — ¿se renombró?"));
        let context = &ui[start.saturating_sub(1200)..(start + 400).min(ui.len())];
        assert!(
            context.contains(marker),
            "`{component_name}` perdió el espaciador flexible que le impedía fijarle el ancho a la \
             ventana. Sus medidas son fijas, así que sin él su ancho MÁXIMO vuelve a ser finito \
             y entrar ahí ENCOGE la ventana maximizada."
        );
    }
}
