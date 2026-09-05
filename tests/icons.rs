//! Toda la iconografía de Play es Phosphor, y ningún glifo se escribe a mano (#627).
//!
//! # Por qué existe
//!
//! El `.pen` declara cada icono con su nombre y su librería
//! (`{"icon": "check-circle", "library": "phosphor"}`): el nombre del icono es
//! parte del diseño, igual que un color. Hasta #627 Play no vendorizaba fuente
//! de íconos y cada uno se dibujaba con el carácter Unicode más parecido que la
//! cmap mapeara —`✓` por `check-circle`, `!` por `warning`, `↻` por
//! `magnifying-glass`— o se reemplazaba por palabras.
//!
//! Ahora la fuente está vendorizada, pero el modo de fallo no se fue: un
//! codepoint mal escrito **no da error**, dibuja un cuadradito. Y hay uno peor
//! que el cuadradito: un codepoint que existe pero es OTRO icono — un `x-circle`
//! donde va un `check-circle` se ve perfecto y dice lo contrario. Por eso la UI
//! nombra iconos (`Icons.check-circle`) y nunca codepoints, y este oráculo ata
//! las tres puntas: el nombre del diseño, el codepoint del release oficial
//! (`fonts/Phosphor.codepoints`) y la cmap de la `.ttf` vendorizada.

mod fonts;
use fonts::{cmap_of, root};

use std::collections::{HashMap, HashSet};

fn read(rel: &str) -> String {
    let p = root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

/// `nombre -> codepoint` del release oficial, vendorizado al lado de la `.ttf`.
fn official_table() -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for line in read("fonts/Phosphor.codepoints").lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(n), Some(h)) = (it.next(), it.next()) else {
            continue;
        };
        out.insert(
            n.to_string(),
            u32::from_str_radix(h, 16).expect("codepoint no hexadecimal"),
        );
    }
    assert!(
        out.len() > 1000,
        "la tabla de Phosphor tiene sólo {} entradas",
        out.len()
    );
    out
}

/// `nombre -> codepoint` tal como los declara el catálogo generado.
fn catalog() -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for line in read("ui/icons.slint").lines() {
        let t = line.trim();
        let Some(t) = t.strip_prefix("out property <string> ") else {
            continue;
        };
        let (name, remaining) = t.split_once(':').expect("propiedad sin `:`");
        let opening_brace = remaining
            .find("u{")
            .expect("el valor no es un escape unicode")
            + 2;
        let closing_brace = remaining[opening_brace..]
            .find('}')
            .expect("escape sin cerrar")
            + opening_brace;
        out.push((
            name.trim().to_string(),
            u32::from_str_radix(&remaining[opening_brace..closing_brace], 16)
                .expect("escape no hexadecimal"),
        ));
    }
    out
}

/// Cada icono del catálogo es EL icono de ese nombre, y la fuente lo dibuja.
#[test]
fn the_catalog_says_the_same_as_the_phosphor_release() {
    let official_codepoints = official_table();
    let cmap = cmap_of(&root().join("fonts/Phosphor.ttf"));
    let cat = catalog();

    assert!(
        cat.len() >= 30,
        "icons.slint declara sólo {} iconos",
        cat.len()
    );
    assert!(
        cmap.len() > 1000,
        "la cmap de Phosphor.ttf salió chica ({})",
        cmap.len()
    );

    for (name, cp) in &cat {
        let expected = official_codepoints.get(name).unwrap_or_else(|| {
            panic!("`{name}` no es un icono de Phosphor 2.1.2 (fonts/Phosphor.codepoints)")
        });
        assert_eq!(
            cp, expected,
            "`{name}` apunta a U+{cp:04X} y el release oficial dice U+{expected:04X}:              es OTRO icono, y en pantalla se ve perfecto"
        );
        assert!(
            cmap.contains(cp),
            "`{name}` (U+{cp:04X}) no está en la cmap de fonts/Phosphor.ttf: sale cuadradito"
        );
    }
}

/// La UI pide iconos que existen. Un `Icons.loquesea` que no está declarado no
/// compila, pero un nombre parecido al que quería (`x` por `x-circle`) sí — y
/// dibuja otra cosa.
#[test]
fn the_ui_only_uses_icons_from_the_catalog() {
    let ui = read("ui/appwindow.slint");
    let declared_icons: HashSet<String> = catalog().into_iter().map(|(n, _)| n).collect();

    // Los comentarios quedan afuera: este archivo y el `.slint` hablan de
    // `Icons.*` en prosa, y una mención no dibuja nada.
    let mut used_icons: Vec<String> = Vec::new();
    for line in ui.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut remaining = line;
        while let Some(i) = remaining.find("Icons.") {
            remaining = &remaining[i + 6..];
            let name: String = remaining
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if !name.is_empty() {
                used_icons.push(name);
            }
        }
    }
    assert!(
        used_icons.len() >= 20,
        "la UI usa sólo {} iconos: ¿se perdió la iconografía?",
        used_icons.len()
    );

    let missing: Vec<&String> = used_icons
        .iter()
        .filter(|n| !declared_icons.contains(*n))
        .collect();
    assert!(
        missing.is_empty(),
        "la UI usa iconos que icons.slint no declara: {missing:?}"
    );
}

/// La familia de iconos se nombra en DOS lugares, y los dos dibujan un glifo.
///
/// Es lo que hace que la regla se cumpla sola: mientras `Tokens.font-icon` viva
/// únicamente dentro de `Icono` —el ícono suelto— y de `PadGlyph` —el botón del
/// mando, que a veces lleva un ícono en vez de una letra—, no hay forma de
/// dibujar un ícono sin pasar por el catálogo. El día que alguien ponga la
/// familia a mano en un `Text` suelto, el glifo vuelve a ser un número escrito
/// de memoria.
#[test]
fn the_icon_family_is_only_named_where_it_is_drawn() {
    let ui = read("ui/appwindow.slint");
    const ICON_RENDERERS: [&str; 2] = ["Icon", "PadGlyph"];

    let mut component_name = "<ninguno>";
    let mut seen: Vec<&str> = Vec::new();
    for line in ui.lines() {
        if let Some(remaining) = line.strip_prefix("component ") {
            component_name = remaining
                .split_whitespace()
                .next()
                .unwrap_or("<sin nombre>");
        }
        if line.contains("Tokens.font-icon") {
            assert!(
                ICON_RENDERERS.contains(&component_name),
                "`Tokens.font-icon` aparece en `{component_name}`: la familia de íconos vive                  SÓLO en {ICON_RENDERERS:?}, que es lo que obliga a que el glifo salga de `Icons.*`"
            );
            seen.push(component_name);
        }
    }
    for c in ICON_RENDERERS {
        assert!(
            seen.contains(&c),
            "`{c}` dejó de dibujar con la familia de íconos"
        );
    }
}

/// Y los caracteres que hacían de icono no vuelven.
///
/// No es una lista de «feos»: son exactamente los que la UI usaba como icono
/// porque no había fuente. Cada uno tiene su reemplazo y ninguno tiene por qué
/// aparecer en un texto — el punto medio, la raya y los puntos suspensivos, que
/// SÍ son tipografía, no están acá.
#[test]
fn no_character_acts_as_an_icon() {
    let ui = read("ui/appwindow.slint");
    // (carácter, con qué se dibuja ahora)
    let forbidden_items = [
        ('✓', "Icons.check / Icons.check-circle"),
        ('↻', "Icons.magnifying-glass / Icons.circle-notch"),
        ('•', "Icons.star / Icons.game-controller"),
        ('×', "Icons.x / Icons.x-circle"),
    ];
    let mut found_items: Vec<String> = Vec::new();
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
            if !inside_string {
                continue;
            }
            if let Some((_, replacement)) = forbidden_items.iter().find(|(p, _)| *p == ch) {
                found_items.push(format!("  {ch:?} en la línea {} — va {replacement}", n + 1));
            }
        }
    }
    assert!(
        found_items.is_empty(),
        "hay caracteres haciendo de icono en appwindow.slint:\n{}",
        found_items.join("\n")
    );
}
