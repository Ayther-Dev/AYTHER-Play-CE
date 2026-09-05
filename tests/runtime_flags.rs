//! Todo flag que Play manda tiene que estar en el `usage` del Runtime (#600).
//!
//! El defecto que esto cierra: el `usage` del Runtime listaba seis flags menos
//! de los que Play le mandaba —`--profile`, `--subsystems`, `--mute-buses`,
//! `--output`, `--core-option`, `--patch`—, así que alguien que corriera
//! `ayther_runtime` a mano para reproducir una sesión no tenía cómo saber que
//! esas opciones existían. Y no fallaba: el Runtime las aceptaba igual. Un
//! contrato que sólo una de las dos puntas conoce se desincroniza en el primer
//! flag nuevo y nadie se entera.
//!
//! Como `tokens_drift.rs`, esto NO ejecuta nada: lee las dos puntas del
//! contrato y exige que digan lo mismo. La punta de Play es `launch_config.rs`
//! (`a.push("--…")`) y la del Runtime es el bloque de `usage` en `runtime_application.cpp` —
//! el texto que ve una persona, no la tabla de parseo, porque el defecto era
//! justamente que el parseo entendía más de lo que el texto contaba.

use std::path::PathBuf;

#[path = "../src/runtime_source.rs"]
mod runtime_source;

fn runtime_source_path(relative_path: &str) -> PathBuf {
    runtime_source::root().join(relative_path)
}

fn read_runtime(relative_path: &str) -> String {
    let path = runtime_source_path(relative_path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", path.display()))
}

/// Un archivo de ESTE crate, por `CARGO_MANIFEST_DIR` y no por una ruta desde
/// la raíz.
///
/// EL DEFECTO QUE ESTO CIERRA, y lo destapó el guardián de fronteras: este
/// oráculo salió copiado de `play/` y seguía leyendo `play/src/…`. O sea que
/// medía los flags del OTRO launcher —el propietario— y pasaba en verde sin
/// haber mirado una línea de CE. Un flag que CE agregara y el Runtime no
/// nombrara habría pasado sin que nadie se enterara, que es exactamente lo que
/// este test existe para atrapar.
///
/// Por ruta relativa al manifiesto no puede volver a pasar: apunta al crate que
/// lo compila, se llame como se llame el directorio.
fn read_own(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

/// Los flags que Play puede llegar a mandarle al Runtime.
///
/// Se sacan del FUENTE y no de un `LaunchConfig` armado a mano: un config de
/// prueba sólo emite los flags de las opciones que se le pusieron, así que una
/// opción nueva que nadie se acordó de activar en el test pasaría inadvertida —
/// que es la clase de agujero que este oráculo existe para tapar.
fn flags_play_sends() -> Vec<String> {
    let mut v = Vec::new();
    for source in ["src/launch_config.rs", "src/spawn.rs"] {
        let s = read_own(source);
        let mut remaining = s.as_str();
        while let Some(i) = remaining.find("\"--") {
            remaining = &remaining[i + 1..];
            let end = remaining.find('"').unwrap_or(remaining.len());
            let flag = &remaining[..end];
            // `--core-option key=value` viaja con su valor pegado en algunos
            // sitios; el contrato es el nombre del flag.
            let flag = flag.split(['=', ' ']).next().unwrap_or(flag).to_string();
            if flag.len() > 2 && !v.contains(&flag) {
                v.push(flag);
            }
        }
    }
    v.sort();
    v
}

/// El bloque de `usage` del Runtime, tal como lo lee una persona.
fn runtime_usage() -> String {
    let source = read_runtime("src/runtime_application.cpp");
    let i = source
        .find("\"Usage: ayther_runtime")
        .expect("el usage del Runtime cambió de forma: no está «Usage: ayther_runtime»");
    // Hasta el cierre del `fprintf`: el usage es una tira de literales
    // concatenados y termina cuando aparece el `);`.
    let end = source[i..]
        .find(");")
        .map(|offset| i + offset)
        .unwrap_or(source.len());
    source[i..end].to_string()
}

#[test]
fn the_runtime_usage_lists_every_flag_play_sends() {
    let usage = runtime_usage();
    let flags = flags_play_sends();

    // No-vacuidad: si el escaneo devolviera nada, el test pasaría sin haber
    // comprobado nada. Al 2026-08-27 Play manda diez flags distintos.
    assert!(
        flags.len() >= 8,
        "el escaneo de flags encontró sólo {} — el oráculo se quedó ciego: {flags:?}",
        flags.len()
    );

    let missing: Vec<&String> = flags
        .iter()
        .filter(|f| !usage.contains(f.as_str()))
        .collect();
    let runtime_main = runtime_source_path("src/runtime_application.cpp");
    assert!(
        missing.is_empty(),
        "Play manda estos flags y el usage del Runtime no los nombra: {missing:?}\n\
             Agregalos a {} — el usage es la única documentación que tiene quien reproduce \
             una sesión a mano.\n\nusage actual:\n{usage}",
        runtime_main.display()
    );
}

/// Y al revés no: el Runtime puede tener flags propias que Play no usa
/// (`--frames`, `--capture-at`, `--probe-core`, el auto-test). Lo que este test
/// sí exige es que el `usage` mencione `--manifest`, porque es el que hace
/// reproducible una sesión y el que el modo seguro (#601) va a necesitar.
#[test]
fn the_usage_names_the_session_manifest() {
    assert!(
        runtime_usage().contains("--manifest"),
        "sin --manifest en el usage, la sesión escrita en disco no tiene \
             cómo volver a lanzarse a mano"
    );
}

#[test]
fn removing_a_required_flag_is_detected() {
    let usage = runtime_usage().replace("--input-map", "removed-input-map");
    let missing: Vec<_> = flags_play_sends()
        .into_iter()
        .filter(|flag| !usage.contains(flag))
        .collect();
    assert_eq!(missing, ["--input-map"]);
}
