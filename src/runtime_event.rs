//! runtime_event.rs — el protocolo `AYTHER_STATUS`, tipado (#600).
//!
//! # Por qué deja de ser `contains()`
//!
//! El runtime emite una línea por transición: `AYTHER_STATUS {json}`. Play la
//! leía buscando SUBCADENAS —`json.contains("\"exit\"")` y un escaneo a mano del
//! campo `savestate`—, y eso falla de tres maneras que no dan error:
//!
//! - Un juego que se llame `exit` en el `game_id` dispara el evento equivocado.
//! - Una ruta con comillas escapadas parte el escaneo a mano por la mitad.
//! - Un evento NUEVO se lee como uno viejo si comparte una subcadena.
//!
//! Ninguna de las tres se ve: el resultado es una sesión que no guarda, o que
//! guarda la ruta cortada. Con `serde_json` el evento es un tipo y lo que no
//! encaja se descarta explícitamente.
//!
//! # `v`, y por qué todavía no se lee
//!
//! El protocolo lleva versión para que un runtime más nuevo pueda decir «esto no
//! lo vas a entender». Hoy no la emite ninguno y no hay dos versiones que
//! distinguir, así que `v` se descarta como cualquier otro campo desconocido:
//! comparar contra un número que nadie manda no rechaza nada, y deja la mentira
//! de que la compatibilidad está chequeada. Cuando el protocolo se mueva, el
//! chequeo entra acá —con su oráculo— y no antes.

use serde::Deserialize;

/// Lo que el runtime dice que le pasa.
///
/// `#[serde(tag = "event")]` es lo que hace que el JSON se despache por su campo
/// `event` en vez de por adivinanza. Un evento desconocido NO es un error del
/// parser: es un runtime más nuevo hablando de algo que este Play no maneja, y
/// descartarlo con nombre es mejor que fallar la línea entera.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum RuntimeEvent {
    /// La sesión arrancó y el juego está en pantalla.
    Ready {
        #[serde(default)]
        game_id: String,
        #[serde(default)]
        has_pack: i32,
        /// #596: el `launch.toml` de esta sesión. Es el hilo entre el proceso y
        /// su manifest.
        #[serde(default)]
        manifest: String,
    },
    /// #600: qué se está jugando, para el overlay y el «ahora jugando».
    NowPlaying {
        #[serde(default)]
        game_id: String,
        #[serde(default)]
        title: String,
    },
    /// #600: algo salió a medias pero la sesión sigue. NO es un error: un
    /// warning que se trate como error corta una partida que estaba andando.
    Warning {
        #[serde(default)]
        reason: String,
        #[serde(default)]
        message: String,
    },
    /// #600: la sesión no puede seguir. Lleva el motivo porque «error» a secas
    /// obliga al jugador a adivinar, y adivinar sobre un juego que no arrancó
    /// termina en «no funciona».
    Error {
        #[serde(default)]
        reason: String,
        #[serde(default)]
        message: String,
    },
    /// La sesión terminó bien.
    Exit {
        #[serde(default)]
        savestate: String,
    },
    /// El auto-test de #? que prueba que el launcher sobrevive a un abort.
    CrashTest,
}

/// Una línea de stdout del runtime, ya clasificada.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLine {
    /// Un evento del protocolo.
    Event(RuntimeEvent),
    /// `AYTHER_STATUS` con algo que no se entiende. Se conserva el texto para
    /// poder loguearlo: una línea del protocolo que no parsea es un síntoma, y
    /// tirarla en silencio esconde justo el caso que hay que investigar.
    Malformed(String),
    /// Cualquier otra cosa: logs humanos del runtime, que Play reenvía.
    Log(String),
}

/// Clasifica UNA línea de stdout.
///
/// Es una función pura sobre `&str` a propósito: el lector de verdad vive en un
/// hilo con un `BufReader`, y probar el protocolo a través de un proceso hijo
/// sería probar el sistema operativo. Acá se prueban los eventos.
pub fn classify(line: &str) -> ParsedLine {
    let Some(json) = line.strip_prefix("AYTHER_STATUS ") else {
        return ParsedLine::Log(line.to_string());
    };
    match serde_json::from_str::<RuntimeEvent>(json.trim()) {
        Ok(e) => ParsedLine::Event(e),
        Err(_) => ParsedLine::Malformed(json.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_warning_retains_its_machine_reason_and_human_message() {
        assert_eq!(
            classify(
                r#"AYTHER_STATUS {"protocol_version":1,"event":"warning","reason":"pack.open_failed","message":"cannot open pack"}"#
            ),
            ParsedLine::Event(RuntimeEvent::Warning {
                reason: "pack.open_failed".into(),
                message: "cannot open pack".into()
            })
        );
    }

    fn ev(s: &str) -> RuntimeEvent {
        match classify(s) {
            ParsedLine::Event(e) => e,
            other_line => panic!("no se entendio: {other_line:?}"),
        }
    }

    /// Cada evento del protocolo se deserializa con sus campos. Es el CA de la
    /// issue: uno por evento, porque el modo de fallo del parser viejo era
    /// justamente confundir uno con otro.
    #[test]
    fn each_event_deserializes_with_its_fields() {
        assert_eq!(
            ev(
                r#"AYTHER_STATUS {"event":"ready","game_id":"sonic2","has_pack":1,"manifest":"D:/s/launch.toml"}"#
            ),
            RuntimeEvent::Ready {
                game_id: "sonic2".into(),
                has_pack: 1,
                manifest: "D:/s/launch.toml".into()
            }
        );
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"now-playing","game_id":"sonic2","title":"Sonic 2"}"#),
            RuntimeEvent::NowPlaying {
                game_id: "sonic2".into(),
                title: "Sonic 2".into()
            }
        );
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"warning","reason":"el pack no trae musica"}"#),
            RuntimeEvent::Warning {
                reason: "el pack no trae musica".into(),
                message: String::new(),
            }
        );
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"error","reason":"el core no cargo"}"#),
            RuntimeEvent::Error {
                reason: "el core no cargo".into(),
                message: String::new(),
            }
        );
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"exit","savestate":"D:/saves/sonic2.bin"}"#),
            RuntimeEvent::Exit {
                savestate: "D:/saves/sonic2.bin".into()
            }
        );
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"crash-test"}"#),
            RuntimeEvent::CrashTest
        );
    }

    /// EL DEFECTO QUE ESTO CIERRA: `contains("\"exit\"")` disparaba el evento
    /// equivocado con cualquier línea que llevara esa subcadena — un `game_id`
    /// que la contenga, un mensaje de warning que la mencione. Ahora el
    /// despacho es por el campo `event` y nada más.
    #[test]
    fn a_substring_no_longer_fires_the_wrong_event() {
        let e = ev(r#"AYTHER_STATUS {"event":"ready","game_id":"exit_the_gungeon","has_pack":0}"#);
        assert!(
            matches!(e, RuntimeEvent::Ready { .. }),
            "un game_id con «exit» adentro no es un exit: {e:?}"
        );

        let e = ev(r#"AYTHER_STATUS {"event":"warning","reason":"no se pudo escribir el exit"}"#);
        assert!(matches!(e, RuntimeEvent::Warning { .. }));
    }

    /// Y el otro: la ruta del savestate se escaneaba a mano buscando comillas.
    /// Una ruta con una comilla escapada partía el escaneo por la mitad y la
    /// sesión guardaba en un lugar que no existe — sin error.
    #[test]
    fn a_path_with_quotes_arrives_whole() {
        let e = ev(r#"AYTHER_STATUS {"event":"exit","savestate":"D:/mis \"saves\"/a.bin"}"#);
        assert_eq!(
            e,
            RuntimeEvent::Exit {
                savestate: "D:/mis \"saves\"/a.bin".into()
            }
        );

        // Y una con backslashes, que es lo normal en Windows.
        let e = ev(r#"AYTHER_STATUS {"event":"exit","savestate":"D:\\saves\\a.bin"}"#);
        assert_eq!(
            e,
            RuntimeEvent::Exit {
                savestate: "D:\\saves\\a.bin".into()
            }
        );
    }

    /// Una línea malformada NO tumba el lector ni se confunde con un evento: se
    /// clasifica como ininteligible **conservando el texto**, porque una línea
    /// del protocolo que no parsea es un síntoma y tirarla en silencio esconde
    /// justo el caso que hay que investigar.
    #[test]
    fn a_malformed_line_is_reported_instead_of_breaking() {
        for malformed_line in [
            r#"AYTHER_STATUS {no es json"#,
            r#"AYTHER_STATUS {"event":"desconocido"}"#,
            r#"AYTHER_STATUS "#,
            r#"AYTHER_STATUS {}"#,
        ] {
            match classify(malformed_line) {
                ParsedLine::Malformed(t) => {
                    assert!(!t.is_empty() || malformed_line.ends_with(' '))
                }
                other_line => panic!("«{malformed_line}» se leyo como {other_line:?}"),
            }
        }
    }

    /// Lo que no lleva el prefijo es un log humano y se reenvía tal cual. Que
    /// sea un caso propio —y no «ininteligible»— es lo que deja distinguir «el
    /// runtime está contando algo» de «el protocolo se rompió».
    #[test]
    fn human_logs_are_told_apart_from_the_protocol() {
        assert_eq!(
            classify("[CoreLoader] cargando gpgx.dll"),
            ParsedLine::Log("[CoreLoader] cargando gpgx.dll".into())
        );
        // Incluso si mencionan el prefijo en el medio: sólo cuenta al principio.
        assert!(matches!(
            classify("[x] vi un AYTHER_STATUS {\"event\":\"exit\"}"),
            ParsedLine::Log(_)
        ));
    }

    /// Los campos que faltan caen a su default en vez de invalidar el evento: un
    /// runtime viejo que no emita `manifest` sigue diciendo `ready`, y perder el
    /// evento entero por un campo nuevo obligaría a actualizar las dos mitades a
    /// la vez.
    #[test]
    fn a_missing_new_field_does_not_invalidate_the_event() {
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"ready","game_id":"x","has_pack":0}"#),
            RuntimeEvent::Ready {
                game_id: "x".into(),
                has_pack: 0,
                manifest: String::new()
            }
        );
        assert_eq!(
            ev(r#"AYTHER_STATUS {"event":"exit"}"#),
            RuntimeEvent::Exit {
                savestate: String::new()
            }
        );
    }
}
