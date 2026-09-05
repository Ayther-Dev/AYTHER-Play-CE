//! Los oráculos del paquete de diagnóstico (#622).

use super::*;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("play-diag-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// **CA de #622**: el paquete dice qué trae y qué NO trae. Uno al que le falta
/// algo sin avisar se lee como uno completo que no explica nada.
#[test]
fn what_is_missing_is_said_instead_of_omitted() {
    let diag = tmp("vacio");
    let p = package("0.1.0", None, &diag);

    assert!(p.text.contains("Sin sesión"), "{}", p.text);
    assert!(p.text.contains("F9"), "y dice CÓMO conseguir el que falta");
    assert!(p.text.contains("Ninguno registrado"));
    assert_eq!(p.sections[0], ("sesión", false));
    assert_eq!(p.sections[2], ("cierre inesperado", false));
    let _ = std::fs::remove_dir_all(&diag);
}

/// El pánico entra al paquete —es justo lo que hay que mandar— y entra
/// REDACTADO, aunque el hook que lo escribió no haya pasado por el log.
#[test]
fn the_last_panic_gets_in_and_goes_redacted() {
    let diag = tmp("panico");
    std::fs::write(
        diag.join("play-panico-2026-08-29.log"),
        r"pánico al abrir D:\Juegos\ROMs\Sonic 2.md: índice fuera de rango",
    )
    .unwrap();
    // Y uno viejo, que no tiene que tapar al nuevo.
    std::fs::write(
        diag.join("play-panico-2020-01-01.log"),
        "un pánico viejísimo",
    )
    .unwrap();

    let p = package("0.1.0", None, &diag);
    assert!(p.text.contains("índice fuera de rango"), "{}", p.text);
    assert!(!p.text.contains("viejísimo"), "sólo el más reciente");
    assert!(!p.text.contains("Sonic"), "redactado: {}", p.text);
    assert!(p.text.contains("<ruta:.md>"), "{}", p.text);
    assert!(
        p.sections
            .iter()
            .any(|(q, has_gamepad)| *q == "cierre inesperado" && *has_gamepad)
    );
    let _ = std::fs::remove_dir_all(&diag);
}

/// Guardar deja EXACTAMENTE lo que se mostró: si el archivo y la pantalla
/// pudieran diferir, «se ve antes de mandarlo» no garantizaría nada.
#[test]
fn what_gets_saved_matches_what_gets_read() {
    let diag = tmp("guardar");
    let p = package("0.1.0", None, &diag);
    let destination = save(&diag, &p.text).unwrap();
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), p.text);
    // Y no se mandó nada: lo único que pasó es un archivo en el disco del
    // jugador, con nombre estable para que lo encuentre.
    assert_eq!(destination.file_name().unwrap(), "diagnostico-play.txt");
    let _ = std::fs::remove_dir_all(&diag);
}
