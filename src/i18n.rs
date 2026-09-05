//! i18n.rs — idioma de la interfaz (#585, PLAY-EP04.2).
//!
//! La spec pide español e inglés desde el MVP (§10) y la UI venía **bilingüe
//! por accidente**: «Install» al lado de «Sin pack — se juega el original», en
//! la misma pantalla.
//!
//! **El msgid está en español.** El equipo, la especificación de producto y
//! `docs/design/play-ce.pen` están en español, así que el español es el idioma
//! base: cuando no hay traducción, se muestra el propio msgid. `lang/en/`
//! traduce al inglés.
//!
//! **Sin gettext en runtime.** Los `.po` los empaqueta `slint-build` dentro del
//! binario (`with_bundled_translations`, ver `build.rs`); la feature `gettext`
//! de Slint quedó descartada porque arrastra `gettext-sys`, que en Windows
//! compila libintl desde fuente con autotools. Los textos que arma Rust usan
//! `i18n_catalog.rs`, generado del MISMO diccionario por
//! `play-ce/tools/gen_translations.py`: la UI y los mensajes de estado no pueden
//! terminar en idiomas distintos.

use std::sync::RwLock;

/// Traduce un texto que arma Rust, con el mismo catálogo que la UI.
///
/// `tr!("Jugando: {}", nombre)` — los marcadores son los de `format!`, pero el
/// **msgid es la plantilla**, no el resultado: así el catálogo tiene una sola
/// entrada por mensaje y el traductor puede mover el argumento de lugar.
#[macro_export]
macro_rules! tr {
    ($msg:literal) => { $crate::i18n::gettext($msg) };
    ($msg:literal, $($arg:expr),+ $(,)?) => {{
        let template = $crate::i18n::gettext($msg);
        $crate::i18n::format_message(&template, &[$(&$arg as &dyn std::fmt::Display),+])
    }};
}

/// El idioma vigente. `RwLock` y no `OnceLock` porque el idioma CAMBIA en
/// caliente: el paso 1 del onboarding (#588) lo elige con la ventana ya
/// abierta, y Ajustes (#599) lo va a cambiar despues. Con un valor de una sola
/// escritura, el jugador elegia ingles y la biblioteca seguia en espanol.
static LANG: RwLock<String> = RwLock::new(String::new());

/// Si hay un bundle de traducción elegido ahora mismo.
static SELECTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// El idioma efectivo de esta corrida (`"es"` mientras nadie llame a `init`).
pub fn lang() -> String {
    let l = LANG.read().map(|s| s.clone()).unwrap_or_default();
    if l.is_empty() { "es".to_string() } else { l }
}

pub fn gettext(msgid: &str) -> String {
    crate::i18n_catalog::lookup(&lang(), msgid)
        .unwrap_or(msgid)
        .to_string()
}

/// Reemplaza cada `{}` por su argumento, en orden. Deliberadamente mínimo:
/// `format!` necesita literales en tiempo de compilación y acá la plantilla
/// viene del catálogo.
pub fn format_message(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut it = args.iter();
    let mut remaining = template;
    while let Some(i) = remaining.find("{}") {
        out.push_str(&remaining[..i]);
        match it.next() {
            Some(a) => out.push_str(&a.to_string()),
            // Menos argumentos que marcadores: es un error del catálogo y se
            // ve en pantalla. Comerlo dejaría una frase incompleta que nadie
            // puede rastrear hasta el .po.
            None => out.push_str("{}"),
        }
        remaining = &remaining[i + 2..];
    }
    out.push_str(remaining);
    out
}

/// Idioma del sistema, por las variables de entorno de siempre.
///
/// En Windows no suelen estar puestas y se cae al español (el idioma base).
/// La detección real del locale del sistema entra con la pantalla de idioma
/// del onboarding (#588), donde además el jugador puede elegir: adivinar mal
/// es peor que preguntar una vez.
fn system() -> String {
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let code = v.split(['_', '.', ':']).next().unwrap_or("").to_lowercase();
            if !code.is_empty() {
                return code;
            }
        }
    }
    "es".into()
}

/// Resuelve el idioma de la interfaz y lo deja fijado para `tr!`.
/// `lang`: `"es"`, `"en"` o `"auto"`. Devuelve el efectivo.
///
/// Se llama al arrancar, antes de leer cualquier texto desde Rust. La parte de
/// Slint va aparte (`aplicar_a_slint`) porque las traducciones EMPAQUETADAS se
/// registran recién cuando existe el primer componente: seleccionarlas antes
/// falla con «There are no bundled translations», que es justo lo que no se ve
/// hasta que alguien abre la app en otro idioma.
pub fn init(lang: &str) -> String {
    let effective_language = if lang == "auto" {
        system()
    } else {
        lang.to_string()
    };
    if let Ok(mut l) = LANG.write() {
        *l = effective_language.clone();
    }
    effective_language
}

/// Cambia el idioma con la ventana ya abierta. Despues hay que llamar a
/// `aplicar_a_slint()` para que la UI se repinte.
pub fn change_language(lang: &str) -> String {
    init(lang)
}

/// Aplica el idioma ya resuelto a los textos de la UI. Llamar DESPUÉS de crear
/// la ventana. Slint re-evalúa los `@tr` de las propiedades vivas, así que
/// esto también es el camino del cambio de idioma en caliente (Ajustes, #599).
pub fn apply_to_slint() {
    // El español es el catálogo base (son los msgid): no hay bundle que elegir.
    if lang() == "es" {
        // El español es el catálogo base: sus textos SON los msgid, así que no
        // hay bundle que elegir. Sólo hay que deseleccionar si antes se había
        // elegido otro idioma —el jugador que vuelve de inglés a español—;
        // hacerlo siempre reentra en el repintado de Slint durante el arranque
        // y la ventana muere con «RefCell already borrowed».
        if SELECTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            let _ = slint::select_bundled_translation("");
        }
        return;
    }
    SELECTED.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = slint::select_bundled_translation(&lang()) {
        eprintln!(
            "[play] no hay traducción para «{}» ({e}); se usa español",
            lang()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_respects_placeholder_order() {
        assert_eq!(
            format_message("Jugando: {}", &[&"Golden Axe"]),
            "Jugando: Golden Axe"
        );
        assert_eq!(format_message("{} de {}", &[&3, &7]), "3 de 7");
        assert_eq!(format_message("sin marcadores", &[]), "sin marcadores");
    }

    #[test]
    fn a_placeholder_without_argument_shows_on_screen_instead_of_vanishing() {
        assert_eq!(format_message("faltan: {} y {}", &[&1]), "faltan: 1 y {}");
    }

    #[test]
    fn spanish_is_the_msgid_and_english_comes_from_the_catalog() {
        // El idioma base no necesita catálogo: se muestra el propio msgid.
        assert_eq!(crate::i18n_catalog::lookup("es", "Créditos"), None);
        assert_eq!(
            crate::i18n_catalog::lookup("en", "Créditos"),
            Some("Credits")
        );
        // Y un idioma que no existe cae al msgid, no a una cadena vacía.
        assert_eq!(crate::i18n_catalog::lookup("de", "Créditos"), None);
    }

    #[test]
    fn status_messages_have_a_translation() {
        // Los textos que arma Rust son los que se olvidan: la UI se revisa a
        // ojo, un `set_status` no. Si alguien agrega un `tr!` y no lo traduce,
        // `gen_translations.py --check` falla en CI — y esto lo fija para los
        // que ya existen.
        for m in [
            "Escaneando…",
            "Jugando: {}",
            "Completo y sin alterar",
            "{} juegos en la biblioteca",
            "sin autor declarado",
        ] {
            assert!(
                crate::i18n_catalog::lookup("en", m).is_some(),
                "«{m}» no tiene traducción al inglés"
            );
        }
    }

    #[test]
    fn from_the_system_normalizes_the_locale_code() {
        // es_AR.UTF-8 → «es»; en-US → «en». Sin esto, un locale con región
        // buscaría un bundle «es_AR» que no existe y caería al idioma base sin
        // decir por qué.
        for (env, expected) in [("es_AR.UTF-8", "es"), ("en_US", "en"), ("en", "en")] {
            let code = env.split(['_', '.', ':']).next().unwrap().to_lowercase();
            assert_eq!(code, expected);
        }
    }
}
