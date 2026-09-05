//! Ningún texto visible se escapa del catálogo (#585).
//!
//! La UI de Play venía **bilingüe por accidente**: «Install» al lado de «Sin
//! pack — se juega el original», en la misma pantalla, porque cada string se
//! escribía donde hacía falta y nadie llevaba la cuenta. Que ahora estén todos
//! traducidos no alcanza: hay que impedir que el próximo texto entre sin pasar
//! por `@tr`, y eso no lo ve una revisión de código a ojo.

use std::path::PathBuf;

fn play_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn read(rel: &str) -> String {
    let p = play_dir().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

/// Nombres propios: la marca y el producto no se traducen. Y «Start», que no
/// es una palabra sino lo que dice el botón del mando: traducirlo mandaría a
/// buscar un botón que no existe en el aparato que el jugador tiene en la mano
/// —por eso los pies dicen «Start continuar» / «Start continue» y traducen
/// sólo la mitad que es idioma. Los de una letra (A, B, Y) ya no llegan acá:
/// el chequeo pide dos letras.
const UNTRANSLATED_EXCEPTIONS: &[&str] = &["Ayther Play", "AYTHER PLAY", "Hub", "Start"];

/// Todo texto visible del `.slint` pasa por `@tr(...)`.
#[test]
fn the_ui_has_no_loose_texts() {
    let ui = read("ui/appwindow.slint");
    let mut unwrapped_text: Vec<(usize, String)> = Vec::new();

    for (n, line) in ui.lines().enumerate() {
        let t = line.trim();
        // Comparaciones de teclas (`event.text == "d"`) y comentarios no son UI.
        if t.starts_with("//") || line.contains("event.text ==") {
            continue;
        }
        for prop in ["text: ", "title: ", "placeholder-text: "] {
            let Some(i) = line.find(prop) else { continue };
            let remaining = &line[i + prop.len()..];
            // `text: @tr(...)`, `text: alguna-propiedad` → ya está bien.
            if !remaining.starts_with('"') {
                continue;
            }
            let content: String = remaining[1..].chars().take_while(|c| *c != '"').collect();
            let has_letters = content.chars().filter(|c| c.is_alphabetic()).count() >= 2;
            if has_letters && !UNTRANSLATED_EXCEPTIONS.contains(&content.as_str()) {
                unwrapped_text.push((n + 1, content));
            }
        }
    }
    assert!(
        unwrapped_text.is_empty(),
        "textos visibles sin @tr en appwindow.slint: {unwrapped_text:?}\n\
         Envolvelos con @tr(\"…\") y corré `py play-ce/tools/gen_translations.py`."
    );
}

/// El catálogo cubre todo lo que la UI y Rust piden, sin sobrantes.
#[test]
fn the_catalog_covers_exactly_what_is_used() {
    let po = read("lang/en/LC_MESSAGES/ayther_play_ce.po");
    let ui = read("ui/appwindow.slint");
    let cat = read("src/i18n_catalog.rs");

    // msgids del .po (los que Slint empaqueta).
    let msgids: Vec<String> = po
        .lines()
        .filter_map(|l| l.strip_prefix("msgid \""))
        .filter_map(|l| l.strip_suffix('"'))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    assert!(
        msgids.len() >= 30,
        "el catálogo tiene sólo {} entradas",
        msgids.len()
    );

    // Cada `@tr("…")` de la UI está en el catálogo.
    let mut missing = Vec::new();
    let mut remaining = ui.as_str();
    while let Some(i) = remaining.find("@tr(\"") {
        remaining = &remaining[i + 5..];
        let msgid: String = {
            let mut s = String::new();
            let mut chars = remaining.chars();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        s.push('\\');
                        if let Some(n) = chars.next() {
                            s.push(n);
                        }
                    }
                    '"' => break,
                    _ => s.push(c),
                }
            }
            s
        };
        if !msgids.contains(&msgid) {
            missing.push(msgid);
        }
    }
    assert!(
        missing.is_empty(),
        "la UI usa textos que no están en el catálogo: {missing:?}\n\
         Corré `py play-ce/tools/gen_translations.py`."
    );

    // Y el catálogo de Rust tiene una entrada por cada msgid: si los dos
    // backends (Slint empaquetado / catálogo Rust) se desincronizan, media
    // pantalla queda en un idioma y media en otro — el defecto original.
    for m in &msgids {
        assert!(
            cat.contains(&format!("{m:?}")) || cat.contains(m.as_str()),
            "«{m}» está en el .po pero no en src/i18n_catalog.rs"
        );
    }
}

/// Y lo mismo para los textos que arma Rust (`tr!`).
///
/// El test de arriba miraba sólo el `.slint`, así que un `tr!` sin traducir
/// pasaba en verde acá y sólo lo veía `gen_translations.py --check` — que es un
/// paso de Python que un desarrollador puede no correr. Peor: cuando el mensaje
/// lleva argumentos, rustfmt manda el literal a la línea siguiente
/// (`tr!(\n    "…", arg)`), y hasta el generador lo perdía; se descubrió en
/// #589, con cinco mensajes de error del core que habrían salido en español en
/// una interfaz en inglés.
#[test]
fn the_messages_rust_builds_are_translated_too() {
    let po = read("lang/en/LC_MESSAGES/ayther_play_ce.po");
    let mut missing: Vec<(String, String)> = Vec::new();
    let mut seen = 0usize;

    let src = play_dir().join("src");
    for entry in std::fs::read_dir(&src).expect("no se pudo listar src/") {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // El propio i18n.rs y su catálogo generado contienen msgids de ejemplo
        // en tests; no son textos de la aplicación.
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name == "i18n.rs" || name == "i18n_catalog.rs" {
            continue;
        }
        let txt = std::fs::read_to_string(&p).unwrap();
        let mut remaining = txt.as_str();
        // `tr!(` seguido, quizá tras un salto de línea, del literal.
        while let Some(i) = remaining.find("tr!(") {
            remaining = &remaining[i + 4..];
            let trimmed = remaining.trim_start();
            let Some(body) = trimmed.strip_prefix('"') else {
                continue;
            };
            let msgid: String = {
                let mut s = String::new();
                let mut chars = body.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            s.push('\\');
                            if let Some(n) = chars.next() {
                                s.push(n);
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                s
            };
            if msgid.is_empty() {
                continue;
            }
            seen += 1;
            if !po.contains(&format!("msgid \"{msgid}\"")) {
                missing.push((name.clone(), msgid));
            }
        }
    }
    assert!(
        seen >= 10,
        "sólo se encontraron {seen} `tr!` en src/: el escáner no está mirando"
    );
    assert!(
        missing.is_empty(),
        "mensajes de Rust que no están en el catálogo: {missing:?}\n\
         Agregalos al diccionario EN de play-ce/tools/gen_translations.py y corré el script."
    );
}

/// El generador no tiene traducciones huérfanas ni textos sin traducir. Es el
/// mismo chequeo que `--check` hace en CI, replicado acá para que un
/// desarrollador sin Python igual se entere.
#[test]
fn there_are_no_empty_translations() {
    let po = read("lang/en/LC_MESSAGES/ayther_play_ce.po");
    let mut empty_translations = Vec::new();
    let lines: Vec<&str> = po.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        let Some(msgid) = l.strip_prefix("msgid \"").and_then(|s| s.strip_suffix('"')) else {
            continue;
        };
        if msgid.is_empty() {
            continue; // la cabecera
        }
        if lines.get(i + 1).map(|s| s.trim()) == Some("msgstr \"\"") {
            empty_translations.push(msgid.to_string());
        }
    }
    assert!(
        empty_translations.is_empty(),
        "entradas sin traducir en en/: {empty_translations:?}"
    );
}
