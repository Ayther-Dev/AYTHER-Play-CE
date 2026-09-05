//! El diseño y la UI no pueden divergir (#584).
//!
//! `play-ce/ui/tokens.slint` se GENERA desde `play-ce/docs/design/play-ce.pen`
//! (`play-ce/tools/gen_tokens.py`) y se commitea. Un archivo generado que nadie
//! verifica se desactualiza en el primer cambio de diseño y nadie se entera
//! hasta que alguien compara una captura con el `.pen` — que es exactamente
//! cómo el `.slint` terminó con 114 colores literales y un acento de facto
//! (`#f0a020`) que ningún documento declaraba.
//!
//! Este test NO re-ejecuta el generador (el build no depende de Python): lee
//! las dos puntas y exige que digan lo mismo.

use std::path::{Path, PathBuf};

fn play_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

/// Cada variable de color/número del `.pen` está en `tokens.slint` con su valor.
#[test]
fn the_slint_tokens_are_the_pen_ones() {
    let pen: serde_json::Value =
        serde_json::from_str(&read(&play_dir().join("docs/design/play-ce.pen")))
            .expect("play-ce.pen no es JSON válido");
    let slint = read(&play_dir().join("ui/tokens.slint"));
    let vars = pen["variables"]
        .as_object()
        .expect("play-ce.pen sin `variables`");

    // Control de no-vacuidad: un .pen recortado pasaría el bucle de abajo sin
    // comprobar nada, que es el modo silencioso de fallar de un guardián.
    assert!(
        vars.len() >= 30,
        "el .pen declara sólo {} variables",
        vars.len()
    );

    let mut checked_count = 0usize;
    for (name, v) in vars {
        let prop = name.trim_start_matches('-');
        match v["type"].as_str() {
            Some("color") => {
                let hex = v["value"].as_str().unwrap();
                let decl = format!("out property <color> {prop}:");
                let line = slint
                    .lines()
                    .find(|l| l.trim_start().starts_with(&decl))
                    .unwrap_or_else(|| {
                        panic!("tokens.slint no declara el color `{prop}` del .pen")
                    });
                assert!(
                    line.contains(hex),
                    "el color `{prop}` difiere: el .pen dice {hex} y tokens.slint dice «{}»",
                    line.trim()
                );
                checked_count += 1;
            }
            Some("number") => {
                let n = v["value"].as_f64().unwrap();
                let decl = format!("out property <length> {prop}:");
                let line = slint
                    .lines()
                    .find(|l| l.trim_start().starts_with(&decl))
                    .unwrap_or_else(|| {
                        panic!("tokens.slint no declara la medida `{prop}` del .pen")
                    });
                assert!(
                    line.contains(&format!("{}px", n as i64)),
                    "la medida `{prop}` difiere: el .pen dice {n}px y tokens.slint dice «{}»",
                    line.trim()
                );
                checked_count += 1;
            }
            _ => {} // las familias tipográficas se comprueban abajo
        }
    }
    assert!(
        checked_count >= 30,
        "sólo se compararon {checked_count} tokens"
    );
}

/// La escala tipográfica sale de los átomos `t-*` del `.pen`, no de números
/// escritos a mano en el `.slint`.
#[test]
fn the_type_scale_comes_from_the_pen_atoms() {
    let pen: serde_json::Value =
        serde_json::from_str(&read(&play_dir().join("docs/design/play-ce.pen"))).unwrap();
    let slint = read(&play_dir().join("ui/tokens.slint"));

    fn search(n: &serde_json::Value, out: &mut Vec<(String, i64)>) {
        if let Some(o) = n.as_object() {
            if o.get("reusable").and_then(|r| r.as_bool()) == Some(true)
                && let (Some(name), Some(size)) = (
                    o.get("name").and_then(|v| v.as_str()),
                    o.get("fontSize").and_then(|v| v.as_i64()),
                )
                && name.starts_with("t-")
            {
                out.push((name.to_string(), size));
            }
            if let Some(ch) = o.get("children").and_then(|c| c.as_array()) {
                for c in ch {
                    search(c, out);
                }
            }
        } else if let Some(a) = n.as_array() {
            for c in a {
                search(c, out);
            }
        }
    }
    let mut atoms = Vec::new();
    search(&pen["children"], &mut atoms);
    assert!(
        atoms.len() >= 7,
        "el .pen tiene {} átomos `t-*` (se esperaban 7)",
        atoms.len()
    );

    for (name, size) in atoms {
        let prop = format!("text-{}", name.trim_start_matches("t-"));
        let decl = format!("out property <length> {prop}:");
        let line = slint
            .lines()
            .find(|l| l.trim_start().starts_with(&decl))
            .unwrap_or_else(|| {
                panic!("tokens.slint no declara `{prop}` (átomo `{name}` del .pen)")
            });
        assert!(
            line.contains(&format!("{size}px")),
            "`{prop}` difiere: el átomo del .pen mide {size}px y tokens.slint dice «{}»",
            line.trim()
        );
    }
}

/// La UI no vuelve a tener colores de marca escritos a mano. Se permiten los de
/// DOMINIO (la paleta por plataforma), que no son tokens de marca — mismo
/// criterio que el Lab con los canales FM/PSG.
#[test]
fn the_ui_has_no_literal_brand_colors() {
    let ui = read(&play_dir().join("ui/appwindow.slint"));
    let palette_start = ui
        .find("pure public function platform-bg")
        .expect("sin paleta de plataforma");
    let palette_end = ui
        .find("pure public function pack-color")
        .expect("sin pack-color");
    let outside_palette: String = format!("{}{}", &ui[..palette_start], &ui[palette_end..]);

    let mut literal_colors: Vec<String> = Vec::new();
    let bytes: Vec<char> = outside_palette.chars().collect();
    for (i, c) in bytes.iter().enumerate() {
        if *c != '#' {
            continue;
        }
        let hex: String = bytes[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() == 6 || hex.len() == 8 {
            literal_colors.push(format!("#{hex}"));
        }
    }
    assert!(
        literal_colors.is_empty(),
        "appwindow.slint volvió a tener colores de marca literales: {literal_colors:?}. \
         Todo color de marca vive en play-ce.pen y llega por `Tokens.*`."
    );

    // Y las fuentes salen del design system, no del sistema operativo.
    assert!(
        ui.contains("default-font-family: Tokens.font-ui"),
        "la ventana no fija la familia tipográfica del design system"
    );
}

/// `Tokens.radius-pill` (999px) DICE «píldora», no la dibuja: femtovg recorta
/// el radio por eje (`rad.min(w/2)` para rx, `rad.min(h/2)` para ry), así que
/// un chip de 120×40 puesto con el token crudo sale con rx=60 y ry=20 — una
/// elipse exacta. La píldora se pide por `Forma.pill(w, h)`, que es la mitad
/// del lado corto. El token se queda en `tokens.slint` porque lo genera el
/// `.pen`; lo que no puede volver es su uso directo en un `border-radius`.
#[test]
fn no_pill_is_drawn_with_the_raw_token() {
    let ui = read(&play_dir().join("ui/appwindow.slint"));

    let raw_values: Vec<(usize, &str)> = ui
        .lines()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with("//") && t.contains("border-radius") && t.contains("Tokens.radius-pill")
        })
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();
    assert!(
        raw_values.is_empty(),
        "hay `border-radius: Tokens.radius-pill` (999px ⇒ elipse) en appwindow.slint: {raw_values:?}. \
         Usá `Shape.pill(self.width, self.height)`."
    );

    // No-vacuidad: el día que alguien borre el helper, el bucle de arriba
    // seguiría pasando sobre un archivo sin píldoras.
    assert!(
        ui.contains("pure public function pill(w: length, h: length) -> length"),
        "appwindow.slint ya no declara `Shape.pill`"
    );
    let usage_count = ui.matches("Shape.pill(").count();
    assert!(
        usage_count >= 7,
        "quedan {usage_count} píldoras por `Shape.pill` (había 7)"
    );
}

/// La marca del chasis: su logo al lado, y el nombre en BLANCO.
///
/// El `.pen` la dibuja así en las diez TopBar y en la portada: `Logo` de 40 en
/// `--brand` y el nombre en `--text-bright`. Estaba sin logo y en el acento —y
/// el acento es del elemento ACTIVO (la pestaña, el chip elegido, el botón
/// primario), así que la marca amarilla competía con ellos en cada pantalla.
/// Quien pone el color de la casa es el logo.
#[test]
fn the_brand_wears_its_logo_and_not_the_accent() {
    let ui = read(&play_dir().join("ui/appwindow.slint"));

    let mut seen_views = 0;
    let mut remaining = ui.as_str();
    while let Some(i) = remaining.find("text: \"AYTHER PLAY\";") {
        remaining = &remaining[i..];
        let block = &remaining[..160.min(remaining.len())];
        assert!(
            block.contains("Tokens.text-bright"),
            "la marca se dibuja con otro color que `text-bright`:
{block}"
        );
        seen_views += 1;
        remaining = &remaining[20..];
    }
    assert!(
        seen_views >= 3,
        "aparecieron {seen_views} marcas: ¿se perdió alguna pantalla?"
    );

    // Y el isologotipo es UN componente, no el trazado copiado: son 200
    // caracteres de path, y la copia que nadie actualiza es la que queda vieja.
    assert!(
        ui.contains("component Logo inherits Path"),
        "no existe el componente del logo"
    );
    assert!(
        ui.matches("commands:").count() == 1,
        "hay más de un trazado de logo en la UI: tiene que salir del componente `Logo`"
    );
    assert!(
        ui.matches("Logo {").count() >= 3,
        "alguna pantalla dejó de dibujar el logo"
    );
}

// ---- #651: la tarjeta mide lo mismo en las dos mitades -------------------

/// El valor de una propiedad de `CardMetrics`, en píxeles.
///
/// Se lee del `.slint` y no se escribe acá por la misma razón que el resto de
/// este archivo: un oráculo con su propia copia de la medida deja de comparar
/// dos cosas y pasa a comparar una consigo misma.
fn card_metric(ui: &str, prop: &str) -> i64 {
    let pat = format!("out property <length> {prop}: ");
    let i = ui
        .find(&pat)
        .unwrap_or_else(|| panic!("`CardMetrics` no declara `{prop}`"));
    let remaining = &ui[i + pat.len()..];
    let end = remaining
        .find("px")
        .unwrap_or_else(|| panic!("`{prop}` no está en px"));
    remaining[..end]
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("`{prop}` no es un número: {}", &remaining[..end]))
}

/// Todos los nodos del `.pen` cuyo nombre empieza con `prefix`.
fn nodes_by_name<'a>(
    value: &'a serde_json::Value,
    prefix: &str,
    out: &mut Vec<&'a serde_json::Value>,
) {
    match value {
        serde_json::Value::Object(object) => {
            if object
                .get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|n| n.starts_with(prefix))
            {
                out.push(value);
            }
            for child in object.values() {
                nodes_by_name(child, prefix, out);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                nodes_by_name(child, prefix, out);
            }
        }
        _ => {}
    }
}

/// La GameCard mide lo mismo en `play-ce.pen` y en `appwindow.slint`.
///
/// EL DEFECTO QUE ESTO CIERRA (#651): las dos mitades se movieron por separado
/// y nadie las cruzó. #629 bajó el alto de la tarjeta en el CÓDIGO (de 474 a
/// 428, porque el semáforo dejó de ocupar fila propia); #640 lo bajó en el
/// `.pen` (de 466 a 420) en un commit que no tocó el `.slint`. Cada cambio
/// tenía su número y su razón, y el resultado fue un documento de diseño que
/// describía una pantalla que no se compilaba — con el ancho divergiendo
/// además desde antes (240 contra 268).
///
/// No falla nada, y ése es el problema: la única forma de enterarse era dibujar
/// una pantalla nueva y comparar a mano contra una captura.
///
/// La decisión de #651 está escrita en la nota 01 del `.pen`: gana el `.pen`
/// salvo donde el código tiene una cuenta que la medida del diseño contradice,
/// que acá es el ancho —seis tarjetas y cinco gaps entran justas en el ancho
/// útil del chasis— y lo que se deriva de él.
#[test]
fn the_card_measures_the_same_in_the_pen_and_in_the_ui() {
    let ui = read(&play_dir().join("ui/appwindow.slint"));
    let pen: serde_json::Value =
        serde_json::from_str(&read(&play_dir().join("docs/design/play-ce.pen")))
            .expect("play-ce.pen no es JSON válido");

    let panel_width = card_metric(&ui, "width");
    let tile_height = card_metric(&ui, "height");
    let padding = card_metric(&ui, "padding");
    let gap = card_metric(&ui, "gap");

    // La cuenta que decide quién manda en el ancho: seis columnas y sus cinco
    // gaps ocupan EXACTAMENTE el ancho útil del chasis. Si deja de dar, la
    // razón escrita en la nota 01 dejó de ser cierta y hay que revisar la
    // decisión, no el número.
    assert_eq!(
        6 * panel_width + 5 * gap,
        1920 - 2 * 96,
        "seis tarjetas de {panel_width} con cinco gaps de {gap} ya no llenan el ancho útil del \
         chasis (1920 menos 96 de safe area por lado). Ése es el argumento por el que el \
         ancho del código le gana al del .pen (#651): sin él, manda el .pen."
    );

    let mut cards = Vec::new();
    nodes_by_name(&pen, "GameCard", &mut cards);
    // No-vacuidad: un `.pen` donde la tarjeta se renombró dejaría el bucle sin
    // nada que comparar y este test en verde para siempre.
    // CE intentionally excludes the managed-service screens. Keep a generous
    // lower bound so the oracle still detects a renamed or truncated design.
    assert!(
        cards.len() >= 12,
        "el .pen tiene sólo {} nodos «GameCard»: o se renombraron o este oráculo dejó de \
         encontrarlos, y en los dos casos hay que arreglarlo y no borrarlo",
        cards.len()
    );

    for c in &cards {
        let id = c["id"].as_str().unwrap_or("?");
        for (prop, expected) in [
            ("width", panel_width),
            ("height", tile_height),
            ("padding", padding),
        ] {
            let v = c[prop].as_i64().unwrap_or_else(|| {
                panic!(
                    "la GameCard {id} del .pen no declara `{prop}` como número: {}",
                    c[prop]
                )
            });
            assert_eq!(
                v, expected,
                "la GameCard {id} del .pen dice {prop} = {v} y `CardMetrics` dice {expected}.\n\
                 Es #651 otra vez: el diseño y el código volvieron a moverse cada uno por su \
                 lado. Decidí en qué mitad va la medida, anotá la razón en la nota 01 del .pen \
                 y movelas JUNTAS."
            );
        }
    }

    // La carátula se DERIVA de la tarjeta: llena de padding a padding. Escrita
    // como número en las dos mitades, es la primera que se despega cuando el
    // ancho cambia.
    let mut covers = Vec::new();
    nodes_by_name(&pen, "Cover", &mut covers);
    let cover_widths: Vec<i64> = covers
        .iter()
        .filter_map(|cover| cover["width"].as_i64())
        .collect();
    assert!(
        cover_widths.contains(&(panel_width - 2 * padding)),
        "ninguna carátula del .pen mide {} de ancho (la tarjeta menos su padding): {cover_widths:?}",
        panel_width - 2 * padding
    );
}
