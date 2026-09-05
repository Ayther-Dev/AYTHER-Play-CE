//! Ningún glifo fuera de las fuentes que Play vendoriza (#592/#593/#627).
//!
//! # Por qué existe
//!
//! Un codepoint que ninguna fuente de `play/fonts/` mapea **no da error**:
//! Slint renderiza un cuadradito. Compila, pasa los tests, se ve bien en el
//! editor — y el defecto sólo aparece mirando la pantalla, que es el peor lugar
//! para que aparezca porque nadie mira todas las pantallas en cada cambio.
//!
//! Ya pasó cuatro veces, cuando Play no tenía fuente de íconos y cada icono se
//! dibujaba con el carácter más parecido: el `⟳` del estado y los `●` de «En
//! juego» y de las estrellas del feedback venían así de antes; el semáforo de
//! #592 nació con `⚠` y `✕`; el menú de #593 nació con seis íconos de los que
//! ninguno existía. Desde #627 la iconografía es Phosphor y tiene su propio
//! oráculo (`icons.rs`); éste cubre lo otro: el TEXTO.
//!
//! # Por qué se lee la fuente y no una lista
//!
//! La primera versión de este test era una lista NEGRA de codepoints conocidos.
//! No sirvió: atrapó los que ya sabíamos y dejó pasar los seis del menú, porque
//! una lista negra sólo cubre lo que a alguien se le ocurrió anotar. Acá se
//! parsea la tabla `cmap` de cada `.ttf` y se pregunta por los que el `.slint`
//! USA. No hay nada que mantener al día: agregar una fuente amplía lo permitido
//! y sacar una lo achica, solo.
//!
//! El lector de `cmap` vive en `tests/fonts/mod.rs`, compartido con
//! `icons.rs`: son dos oráculos de la misma pregunta —qué sabe dibujar la
//! fuente— y un parser copiado se arregla en una sola de las copias.

mod fonts;
use fonts::{coverage, root};

/// Los caracteres no-ASCII que el `.slint` pone DENTRO de comillas, con su línea.
///
/// Sólo literales: un carácter en un comentario no se dibuja, y este mismo
/// archivo tiene que poder nombrar los que están prohibidos.
fn used_in_literals(ui: &str) -> Vec<(char, usize)> {
    let mut out = Vec::new();
    for (n, line) in ui.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut inside_string = false;
        for ch in line.chars() {
            if ch == '"' {
                inside_string = !inside_string;
                continue;
            }
            if inside_string && !ch.is_ascii() {
                out.push((ch, n + 1));
            }
        }
    }
    out
}

#[test]
fn no_glyph_outside_the_fonts() {
    let ui = std::fs::read_to_string(root().join("ui/appwindow.slint")).unwrap();
    let covered_codepoints = coverage();

    let mut missing: Vec<(char, usize)> = used_in_literals(&ui)
        .into_iter()
        .filter(|(c, _)| !covered_codepoints.contains(&(*c as u32)))
        .collect();
    missing.sort_by_key(|(c, l)| (*c as u32, *l));
    missing.dedup_by_key(|(c, _)| *c as u32);

    assert!(
        missing.is_empty(),
        "appwindow.slint usa {} caracter(es) que NINGUNA fuente de play/fonts/ mapea \
         — se renderizan como cuadraditos, sin dar error:\n{}",
        missing.len(),
        missing
            .iter()
            .map(|(c, l)| format!("  U+{:04X} {c:?} (línea {l})", *c as u32))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// El parser tiene que encontrar lo que sabemos que está. Sin esto, un bug que
/// devolviera un conjunto vacío haría pasar el test de arriba SIEMPRE — que es
/// el modo de fallo clásico de un oráculo que compara contra algo que él mismo
/// calcula.
#[test]
fn the_cmap_reader_finds_what_it_has_to_find() {
    let covered_codepoints = coverage();
    for (c, name) in [
        ('A', "latina basica"),
        ('ñ', "eñe"),
        ('á', "vocal acentuada"),
        ('·', "punto medio"),
        ('…', "puntos suspensivos"),
        ('→', "flecha"),
        ('✓', "check"),
        ('×', "por"),
        ('↻', "recargar"),
        ('•', "bolita"),
    ] {
        assert!(
            covered_codepoints.contains(&(c as u32)),
            "el lector no encontró {c:?} ({name}), que sí está en IBM Plex"
        );
    }
    // Y que NO invente: estos no están en ninguna de las siete.
    for c in ['⚠', '✕', '★', '⚙', '✎'] {
        assert!(
            !covered_codepoints.contains(&(c as u32)),
            "el lector dice que {c:?} está, y no está: rangos mal leídos"
        );
    }
}
