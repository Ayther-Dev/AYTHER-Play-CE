//! Los oráculos del log (#621).
//!
//! El que da nombre a la issue es `ninguna_ruta_del_jugador_sale_en_el_log`.
//! El que impide que vuelva es `ningun_print_suelto_imprime_una_ruta`: lee el
//! propio fuente, porque la CA no es «hoy no se filtra» sino «un `eprintln!`
//! nuevo no puede filtrar por olvido».

use super::*;

/// **CA de #621**: ni la ruta ni el nombre de la ROM salen en la línea escrita.
#[test]
fn no_player_path_shows_up_in_the_log() {
    let cases = [
        r"no se pudo abrir D:\Juegos\ROMs\Sonic 2 (mi copia).md",
        r"escaneando C:\Users\david\Mis Documentos\roms",
        r"guardado en \\NAS\respaldos\ayther\partida.srm",
        "leyendo /home/david/roms/Golden Axe.md",
    ];
    for c in cases {
        let r = redact(c);
        for forbidden in [
            "david",
            "Sonic",
            "Golden",
            "Juegos",
            "NAS",
            "Mis Documentos",
            "roms",
        ] {
            assert!(
                !r.contains(forbidden),
                "«{forbidden}» sobrevivió a la redacción de «{c}» → «{r}»"
            );
        }
        assert!(r.contains('<'), "y algo tiene que quedar en su lugar: {r}");
    }
}

/// Lo que SÍ se conserva: que había una ruta y de qué tipo era. Con eso se
/// diagnostica; sin eso el log dice «falló» y nada más.
#[test]
fn redaction_keeps_what_helps_diagnose() {
    assert_eq!(
        redact(r"no se pudo abrir D:\Juegos\Sonic.md"),
        "no se pudo abrir <ruta:.md>"
    );
    assert_eq!(
        redact("cargando Sonic 2.md"),
        "cargando Sonic <archivo:.md>"
    );
    // Una carpeta sin extensión se redacta igual, sin inventarle una.
    assert_eq!(redact(r"escaneando C:\Juegos\roms"), "escaneando <ruta>");
}

/// Una URL no es del disco de nadie: sin ella, un problema de red no se
/// diagnostica.
#[test]
fn urls_stay_whole() {
    let r = redact("no se pudo consultar http://localhost:3001/v1/catalogo?q=sonic");
    assert!(
        r.contains("http://localhost:3001/v1/catalogo?q=sonic"),
        "{r}"
    );
}

/// Un `.ay` es contenido publicado, con nombre público: redactarlo haría
/// ilegible el log de una instalación y no protege a nadie.
#[test]
fn a_pack_id_is_not_redacted() {
    let r = redact("instalado sprites-hd.ay");
    assert!(r.contains("sprites-hd.ay"), "{r}");
}

/// El nivel sale del entorno o de la configuración, sin recompilar, y el
/// entorno gana: quien diagnostica no debería tener que editar el TOML de la
/// máquina de otro.
#[test]
fn the_level_is_chosen_without_recompiling() {
    for (t, n) in [
        ("error", Level::Error),
        ("aviso", Level::Warn),
        ("info", Level::Info),
        ("detalle", Level::Verbose),
        ("DEBUG", Level::Verbose),
    ] {
        assert_eq!(Level::of(t), Some(n), "{t}");
    }
    assert_eq!(Level::of("cuando quiera"), None);
    assert!(Level::Error < Level::Verbose, "el orden es cuánto se dice");

    configure("detalle");
    assert_eq!(level(), Level::Verbose);
    configure("");
    assert_eq!(level(), Level::Info, "sin configuración, el default");
}

/// **La regla estructural**: ningún `print` suelto del código imprime una ruta.
///
/// Lee los fuentes en vez de confiar en la disciplina. Es el mismo patrón que
/// los otros oráculos de contrato del repo: la garantía no es que hoy esté
/// bien, es que mañana no se pueda romper sin que algo se ponga rojo.
#[test]
fn no_stray_print_prints_a_path() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offending_lines = Vec::new();
    walk(&dir, &mut |file, text| {
        // Este módulo es el que redacta: acá `display()` es legítimo.
        if file.ends_with("logging.rs") || file.contains("registro") {
            return;
        }
        // Lo que hay debajo de `#[cfg(test)]` no viaja en el binario del
        // jugador: un test que imprime la ruta de su directorio temporal no
        // filtra nada de nadie.
        let production_source = text.split("#[cfg(test)]").next().unwrap_or(text);
        for (n, l) in production_source.lines().enumerate() {
            let l = l.trim();
            if !(l.contains("println!") || l.contains("eprintln!")) {
                continue;
            }
            if l.contains(".display()") {
                offending_lines.push(format!("{file}:{}: {l}", n + 1));
            }
        }
    });
    assert!(
        offending_lines.is_empty(),
        "estas líneas imprimen una ruta sin pasar por el log que redacta \
         (usá log_info!/log_aviso!/log_error!):\n{}",
        offending_lines.join("\n")
    );
}

fn walk(dir: &std::path::Path, f: &mut impl FnMut(&str, &str)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if let Ok(t) = std::fs::read_to_string(&p) {
                f(&name, &t);
            }
        }
    }
}

/// Un pánico sin archivo ni línea no se puede diagnosticar, y ese archivo es
/// CÓDIGO — no la biblioteca del jugador. Se va la carpeta, que sí lo delata
/// (la del registro de cargo lleva su nombre de usuario); se quedan el nombre y
/// la línea, que es todo lo que sirve.
#[test]
fn what_makes_a_panic_diagnosable_survives_it() {
    let r =
        redact(r"panicked at C:\Users\david\.cargo\registry\src\slint-1.16\ventana.rs:1584:32:");
    assert!(!r.contains("david"), "la carpeta se va: {r}");
    assert!(r.contains("ventana.rs"), "el archivo se queda: {r}");
    assert!(r.contains("1584"), "y la línea también: {r}");
}
