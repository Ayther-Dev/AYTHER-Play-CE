//! no_managed_services.rs — CE no habla con ningún servicio, y no lo nombra donde el
//! jugador lo lee (#676).
//!
//! # Por qué hace falta un oráculo y no alcanza con haberlo sacado
//!
//! `play-ce` nació copiando `play/` y quitándole el Hub. Eso se hace una vez;
//! lo que no se hace una vez es *mantenerlo hecho*. Las dos ediciones comparten
//! casi todo el código, así que arreglar un defecto en una y portarlo a la otra
//! va a ser la operación de todos los días — y la forma natural de portarlo es
//! copiar el bloque entero, con su `hub_client` adentro. Nadie lo va a hacer a
//! propósito: va a entrar en un `match` que se copió porque el resto del brazo
//! era el arreglo.
//!
//! El daño no es un `cargo build` roto —eso se ve—, sino lo que sí compila: una
//! fila de Ajustes que no hace nada, un botón que dice «Instalar» y no tiene de
//! dónde bajar, un candado que en realidad sólo dice «el archivo no está
//! corrupto». Todo eso pasa el compilador y falla en el sillón.
//!
//! # Las dos mitades que mira, y la que NO mira
//!
//! 1. **Que no haya con qué hablar.** Ni las dependencias del servicio, ni un
//!    módulo suyo, ni sus símbolos. Es la mitad estructural.
//! 2. **Que ningún texto de la interfaz lo nombre.** Se mira el catálogo de
//!    traducción, que es exactamente el conjunto de frases que el jugador puede
//!    llegar a leer — ni una más, ni una menos.
//!
//! Lo que **no** mira son los comentarios. Explicar la frontera —«en AYTHER
//! Play esto lo contesta el Hub; acá no hay a quién preguntarle»— es la mitad
//! valiosa de esta separación, y un oráculo que prohibiera la palabra borraría
//! justamente la explicación de por qué el código de al lado no está.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

/// Todos los `.rs` de `src/`, con su nombre relativo.
fn sources() -> Vec<(String, String)> {
    fn gather(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                gather(&p, base, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, read(&p)));
            }
        }
    }
    let src = root().join("src");
    let mut v = Vec::new();
    gather(&src, &src, &mut v);
    v
}

/// Los msgid del catálogo: el conjunto exacto de frases que la interfaz muestra.
fn interface_texts() -> Vec<String> {
    read(&root().join("lang/ayther_play_ce.pot"))
        .lines()
        .filter_map(|l| l.strip_prefix("msgid \""))
        .filter_map(|l| l.strip_suffix('"'))
        .filter(|m| !m.is_empty())
        .map(|m| m.to_string())
        .collect()
}

/// LO QUE NO SE PUEDE COMPILAR CONTRA UN SERVICIO QUE NO EXISTE.
///
/// Las cuatro dependencias que se fueron **son** la diferencia entre las dos
/// ediciones, escrita donde se compila. Que la lista sea más corta no es una
/// casualidad ni una optimización: `reqwest` es hablar con un servicio,
/// `ed25519-dalek` es juzgar una firma que sólo un registro de claves vuelve
/// significativa, `httpdate` era el reloj lógico de los guardados en la nube y
/// `windows-sys` era el almacén del token de sesión.
#[test]
fn the_manifest_brings_nothing_to_talk_to_a_service_with() {
    let toml = read(&root().join("Cargo.toml"));
    // Sólo las líneas que declaran algo: los comentarios de este mismo archivo
    // nombran las cuatro para explicar por qué no están.
    let dependency_declarations: String = toml
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for dependency in ["reqwest", "ed25519", "httpdate", "windows-sys"] {
        assert!(
            !dependency_declarations.contains(dependency),
            "`{dependency}` volvió a las dependencias de CE. Si hace falta, la pregunta no es \
             cómo agregarlo: es si lo que se está portando pertenece a esta edición (ADR-005)."
        );
    }
}

/// NINGÚN MÓDULO DEL HUB, NI SUS SÍMBOLOS.
///
/// Es la forma en que un puerto de un arreglo se trae medio Hub de arrastre: el
/// brazo del `match` que se copia menciona `updater::` o `credenciales::`, y
/// para que compile alguien trae el módulo. Ninguno de estos existe acá, y el
/// día que uno aparezca tiene que ser una decisión y no un efecto secundario.
#[test]
fn there_is_no_hub_module_or_symbol() {
    let src = root().join("src");
    for m in [
        // Ambas grafias: los nombres ingleses (post-normalizacion ES->EN) y
        // los espanoles viejos, por si un port llega desde un checkout viejo.
        "hub",
        "hub_client",
        "trust",
        "confianza",
        "credentials",
        "credenciales",
        "telemetry",
        "telemetria",
        "updater",
        "cloud",
        "nube",
        "login",
        "pack_page",
        "pack_ficha",
        "report",
        "reporte",
        "sram_sync",
    ] {
        let f = src.join(format!("{m}.rs"));
        assert!(
            !f.exists(),
            "volvió el módulo `{m}`: es del Hub, y CE no tiene con quién hablar (ADR-005)"
        );
    }

    // Y los símbolos, para el caso en que el módulo se pegue adentro de otro.
    for (file, text) in sources() {
        for symbol in ["reqwest::", "ed25519_dalek", "httpdate::", "windows_sys::"] {
            assert!(
                !text.contains(symbol),
                "`{file}` usa `{symbol}`: CE no habla con ningún servicio ni juzga \
                 la procedencia de un pack (ADR-005, `verifier.rs`)"
            );
        }
    }
}

/// NINGUNA FRASE DE LA INTERFAZ NOMBRA LO QUE ESTA EDICIÓN NO TIENE.
///
/// **Es la mitad que se ve.** Un texto sobreviviente —«Cuenta del Hub», «Se
/// sincroniza con la nube»— no rompe nada: se dibuja perfecto, se traduce, pasa
/// todos los demás oráculos, y le promete al jugador algo que este binario no
/// puede hacer. El que lo lee no tiene forma de saber que no es un error suyo.
///
/// Se mira el catálogo y no el `.slint`, y no es lo mismo: el catálogo es el
/// conjunto EXACTO de frases que la interfaz puede llegar a mostrar, incluidas
/// las que arma Rust y no están en ningún archivo de UI. Un texto que se le
/// escape a este test es un texto que además está sin traducir.
#[test]
fn no_interface_text_names_the_hub() {
    let texts = interface_texts();
    assert!(
        texts.len() > 100,
        "sólo {} textos en el catálogo: el lector se rompió y este test no está midiendo nada",
        texts.len()
    );
    for t in &texts {
        assert!(
            !forbidden(t),
            "la interfaz dice «{t}», y eso nombra algo que CE no tiene. Si el texto es \
             correcto, lo que hay que revisar es la feature (ADR-005)."
        );
    }
}

/// Una frase de la interfaz que promete lo que CE no puede dar.
///
/// Vive en una función para que el test de abajo pueda comprobar que el oráculo
/// ve el defecto: un chequeo que nunca vio uno pasa en verde tanto si mide como
/// si no.
fn forbidden(text: &str) -> bool {
    let t = text.to_lowercase();
    // «Hub» como palabra suelta: `hubo` y `hubiera` no cuentan, y son comunes.
    let names_managed_service = t.split(|c: char| !c.is_alphanumeric()).any(|p| p == "hub");
    names_managed_service
        || t.contains("la nube")
        || t.contains("telemetr")
        || t.contains("iniciar sesión")
        || t.contains("cerrar sesión")
}

/// Y EL ORÁCULO VE EL DEFECTO.
///
/// Las frases de abajo son las que de verdad estaban en la edición paga: si
/// alguna volviera con un port, el test de arriba tiene que caerse. Y las de
/// después son las que NO puede señalar — «hubo» aparece en el balance de la
/// sesión, y un oráculo con falsos positivos es uno que alguien desactiva.
#[test]
fn the_oracle_sees_a_phrase_that_promises_too_much() {
    for t in [
        "Cuenta del Hub",
        "Sincronizar con la nube",
        "Estadísticas de uso", // no: es el texto de la fila, sin la palabra
        "Borrar mis copias de la nube",
        "El Hub no responde",
        "Iniciar sesión en el Hub",
    ] {
        // «Estadísticas de uso» sola no alcanza —no nombra nada—, así que se
        // mira con la frase que la acompañaba en la pregunta de #623.
        let t = if t == "Estadísticas de uso" {
            "Lo que se manda está escrito en docs/telemetria.md."
        } else {
            t
        };
        assert!(forbidden(t), "el oráculo dejó pasar «{t}»");
    }

    for t in [
        "No hubo guardados en esta sesión",
        "Volver a ver la bienvenida",
        "Sin pack: se juega el original, completo.",
        "Perfil de salida",
    ] {
        assert!(
            !forbidden(t),
            "el oráculo señala «{t}», que no promete nada de más"
        );
    }
}
