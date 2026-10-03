//! pack_details.rs — la FICHA del pack: qué es, quién dice haberlo hecho y si
//! sirve (#312, adaptado a CE por ADR-005).
//!
//! El jugador tiene que poder entender qué está por instalar o jugar **antes**
//! de arrancarlo. La ficha usa el LECTOR DEL CORE
//! (`ayther_core::archive_vfs`), que es el mismo que usa el motor: dos lectores
//! del mismo formato se separan, y el borde que se separa es el que decide si
//! un pack se acepta.
//!
//! ## Lo declarado y lo COMPROBADO no son lo mismo
//!
//! Es el riesgo que anota la issue —«mostrar como verificado contenido que no
//! lo está»— y por eso la ficha los mantiene en campos distintos:
//!
//!   · **Declarado** — lo que el manifest DICE: autor, versión, licencia,
//!     perfiles, sistemas. Cualquiera puede escribir lo que quiera ahí.
//!   · **Comprobado** — lo que se pudo AUDITAR: que el archivo esté completo y
//!     que sus hashes cierren, y que sea compatible con la ROM que el jugador
//!     tiene.
//!
//! ## Y en CE hay una tercera columna que está VACÍA
//!
//! La de la procedencia. Play la llena consultando el registro de claves del
//! Hub; CE no tiene a quién consultarle, así que no la muestra —ni con un
//! candado, ni con un «no se pudo verificar» que se leería como un problema del
//! pack. El campo `auditoria` es lo único que hay, y dice lo que el archivo
//! puede decir de sí mismo. Ver el comentario de `verifier.rs`.
//!
//! ## Compatibilidad
//!
//! No se adivina comparando strings: se le pregunta a `pack_validate` del core
//! —el mismo validador que corre el Hub al recibir un pack (HUB-EP03.2) y el
//! que `ay_pack validate` le muestra al autor. Si el autor lo vio pasar, el
//! jugador no debería ver otra cosa.

#[cfg(test)]
use ayther_core::archive_vfs::AyArchive;
use ayther_core::pack_validate::{self, SessionCtx, Severity};

use crate::verifier::Audit;

#[cfg(test)]
use std::path::Path;

/// Un hallazgo del validador, ya listo para mostrar.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// `error` · `advertencia` · `sugerencia` — el nivel, para la UI.
    pub level: &'static str,
    /// Código estable (`rom.mismatch`, `meta.license`…): una UI puede decidir
    /// con él sin parsear castellano.
    pub code: String,
    pub message: String,
}

/// Todo lo que la pantalla necesita, ya resuelto.
#[derive(Debug, Clone)]
pub struct PackDetails {
    // -- Declarado en el manifest -------------------------------------------
    pub name: String,
    pub version: String,
    pub game_id: String,
    pub author: String,
    pub contributors: Vec<String>,
    pub description: String,
    pub license: String,
    /// (id, nombre) de cada perfil que ofrece.
    pub profiles: Vec<(String, String)>,
    /// Subsistemas que declara traer, por nombre canónico.
    pub systems: Vec<String>,
    /// `false` = el pack no declara `[systems]`, que NO es lo mismo que declarar
    /// una lista vacía (#294). Sin esta distinción la ficha diría «no trae nada»
    /// sobre un pack que trae de todo y es viejo.
    pub systems_declared: bool,

    // -- Comprobado ----------------------------------------------------------
    /// Lo único que CE puede afirmar del archivo: que está completo y que sus
    /// hashes cierran. **No** hay un campo de procedencia, y no es un olvido.
    pub audit: Audit,
    /// Créditos por persona, del `credits.toml` (#309/#314).
    pub credits: Vec<crate::pack_meta::CreditLine>,
    /// Lo que dijo el validador del core, con la ROM del jugador si se conoce.
    pub findings: Vec<Finding>,
    /// Hay un error que impide JUGARLO. No todo error del validador lo es —
    /// ver `impide_jugar`.
    pub incompatible: bool,
    /// #595 — el grado del validador, tal cual, y su motivo.
    ///
    /// La ficha lo trae para poder decir LO MISMO que la tarjeta de la
    /// biblioteca. Antes cada superficie derivaba su propia frase de los mismos
    /// hallazgos, y dos textos distintos sobre el mismo pack se leen como dos
    /// diagnósticos: el jugador no sabe a cuál creerle.
    pub grade: &'static str,
    pub grade_reason: String,
    /// Tamaño del archivo, para la ficha.
    pub size_bytes: u64,
}

impl Default for PackDetails {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            game_id: String::new(),
            author: String::new(),
            contributors: Vec::new(),
            description: String::new(),
            license: String::new(),
            profiles: Vec::new(),
            systems: Vec::new(),
            systems_declared: false,
            // Una ficha a medio armar no puede decir «completo» por omisión: el
            // default es el estado que no afirma NADA bueno del archivo.
            audit: Audit::InvalidPack(String::new()),
            credits: Vec::new(),
            findings: Vec::new(),
            incompatible: false,
            grade: "",
            grade_reason: String::new(),
            size_bytes: 0,
        }
    }
}

impl PackDetails {
    /// ¿Se puede jugar? Un pack con errores del validador no arranca bien, y
    /// enterarse en la ficha es mejor que enterarse en la pantalla negra.
    pub fn playable(&self) -> bool {
        !self.incompatible && self.audit.is_installable()
    }

    /// Una línea para el resumen de la ficha, sin adornos.
    pub fn compat_label(&self) -> String {
        if !self.audit.is_installable() {
            format!("No se puede usar: {}", self.audit.word().to_lowercase())
        } else if self.incompatible {
            let n = self
                .findings
                .iter()
                .filter(|f| f.level == "error" && blocks_play(&f.code))
                .count();
            format!("Incompatible: {n} problema(s) que impiden jugarlo")
        } else if self.grade == "experimental" {
            format!("Compatibilidad pendiente: {}", self.grade_reason)
        } else if self.findings.iter().any(|f| f.level == "advertencia") {
            "Compatible, con avisos".to_string()
        } else {
            "Compatible".to_string()
        }
    }
}

/// Arma la ficha de un `.ay`.
///
/// `rom_crc32` es el de la ROM que el jugador tiene, en hexadecimal como lo
/// guarda la biblioteca. Cuando se conoce, el validador puede contestar si el
/// pack es para ESE juego; cuando no, lo dice en vez de darlo por bueno — un
/// «compatible» sin haber comparado nada es exactamente lo que la issue anota
/// como riesgo.
#[cfg(test)]
pub fn read(pack_path: &Path, rom_crc32: Option<&str>) -> PackDetails {
    if let Some(repo) = crate::pack_cache::global() {
        return match repo.inspect(
            pack_path,
            &std::sync::atomic::AtomicBool::new(false),
            &mut |_, _| {},
        ) {
            Ok(snapshot) => from_snapshot(
                &snapshot,
                &SessionCtx {
                    rom_crc32: rom_crc32
                        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()),
                    ..Default::default()
                },
            ),
            Err(error) => PackDetails {
                audit: Audit::InvalidPack(error),
                ..Default::default()
            },
        };
    }
    let mut d = PackDetails {
        size_bytes: std::fs::metadata(pack_path).map(|m| m.len()).unwrap_or(0),
        ..Default::default()
    };

    // -- Lo declarado, por el lector del core --------------------------------
    let path = pack_path.to_string_lossy().to_string();
    match AyArchive::open_verbose(&path) {
        Ok(pack) => {
            // `meta` es el `[pack]` YA TIPADO por el lector del core. Re-parsear
            // el manifest acá sería una segunda implementación del mismo TOML —
            // exactamente lo que esta ficha vino a sacar del launcher.
            let m = &pack.meta;
            d.game_id = m.game_id.clone();
            d.name = m.name.clone();
            d.version = m.version.clone();
            d.author = m.author.clone().unwrap_or_default();
            d.description = m.description.clone().unwrap_or_default();
            d.license = m.license.clone().unwrap_or_default();
            d.contributors = m.contributors.clone();

            d.systems_declared = pack.has_system("sprites").is_some();
            for s in [
                "sprites",
                "metasprites",
                "tiles",
                "planes",
                "ui",
                "music",
                "sfx",
                "shaders",
            ] {
                if pack.has_system(s) == Some(true) {
                    d.systems.push(s.to_string());
                }
            }
            for p in pack.profiles() {
                let name = p.name.clone().unwrap_or_else(|| p.id.clone());
                d.profiles.push((p.id.clone(), name));
            }
        }
        Err(_) => {
            // No abrió. La ficha sigue: el validador de abajo va a decir por qué,
            // y una pantalla vacía sin motivo es peor que una con el motivo.
        }
    }
    if d.name.is_empty() {
        d.name = pack_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }

    // -- Lo comprobado -------------------------------------------------------
    // La MISMA auditoría que corre la instalación desde el disco. Que la ficha y
    // el instalador contesten por caminos distintos a «¿este archivo está
    // entero?» es cómo se termina ofreciendo instalar algo que después se
    // rechaza a sí mismo.
    d.audit = crate::verifier::audit(pack_path);
    d.credits = crate::pack_meta::credits(pack_path);

    // El MISMO validador que corre el Hub al recibir un pack y el que
    // `ay_pack validate` le mostró al autor: si él lo vio pasar, el jugador no
    // debería ver otra cosa.
    let mut ctx = SessionCtx::default();
    if let Some(crc) = rom_crc32
        && let Ok(v) = u32::from_str_radix(crc.trim_start_matches("0x"), 16)
    {
        ctx.rom_crc32 = Some(v);
    }
    // #595: el GRADO sale de `compat_grade`, la única implementación de la
    // matriz (#530) — la misma que contesta la tarjeta de la biblioteca, el SDK
    // y el Hub. Acá se pide entero porque la ficha muestra además el informe.
    let verdict = pack_validate::compat_grade(&path, &ctx);
    d.grade = verdict.grade.id();
    d.grade_reason = verdict.reason.clone();
    let rep = verdict.report;
    for f in &rep.findings {
        d.findings.push(Finding {
            level: match f.severity {
                Severity::Error => "error",
                Severity::Warning => "advertencia",
                Severity::Info => "sugerencia",
            },
            code: f.code.to_string(),
            message: f.message.clone(),
        });
    }
    // NO todo error del validador impide jugar, y meterlos en la misma bolsa
    // le negaría al jugador un pack que funciona perfectamente.
    d.incompatible = d
        .findings
        .iter()
        .any(|f| f.level == "error" && blocks_play(&f.code));
    d
}

/// Builds a session view without reopening the pack or repeating its audit.
pub fn from_snapshot(snapshot: &crate::pack_cache::Snapshot, ctx: &SessionCtx<'_>) -> PackDetails {
    let verdict = pack_validate::grade_report(
        pack_validate::validate_metadata(&snapshot.metadata, ctx),
        ctx,
    );
    let mut d = PackDetails {
        size_bytes: snapshot.revision.bytes,
        audit: snapshot
            .audit
            .clone()
            .unwrap_or_else(|| Audit::InvalidPack("Comprobación pendiente".into())),
        grade: verdict.grade.id(),
        grade_reason: verdict.reason,
        credits: snapshot
            .metadata
            .texts
            .get("credits.toml")
            .map(|s| crate::pack_meta::credits_from_bytes(s.as_bytes()))
            .unwrap_or_default(),
        findings: verdict
            .report
            .findings
            .into_iter()
            .map(|f| Finding {
                level: match f.severity {
                    Severity::Error => "error",
                    Severity::Warning => "advertencia",
                    Severity::Info => "sugerencia",
                },
                code: f.code.to_owned(),
                message: f.message,
            })
            .collect(),
        ..Default::default()
    };
    if let Some(pack) = &snapshot.catalog {
        d.name.clone_from(&pack.meta.name);
        d.version.clone_from(&pack.meta.version);
        d.game_id.clone_from(&pack.meta.game_id);
        d.author = pack.meta.author.clone().unwrap_or_default();
        d.description = pack.meta.description.clone().unwrap_or_default();
        d.license = pack.meta.license.clone().unwrap_or_default();
        d.contributors.clone_from(&pack.meta.contributors);
        d.systems_declared = pack.systems_declared;
        d.systems.clone_from(&pack.systems.included);
        d.profiles = pack
            .profiles
            .iter()
            .map(|p| (p.id.clone(), p.name.clone().unwrap_or_else(|| p.id.clone())))
            .collect();
    }
    d.incompatible = d
        .findings
        .iter()
        .any(|f| f.level == "error" && blocks_play(&f.code));
    d
}

/// ¿Este error impide JUGAR el pack, o sólo publicarlo?
///
/// La distinción no es cosmética. `provenance.missing` —una licencia que exige
/// atribución sin `credits.toml`— es un error grave **para el autor**: afirma un
/// permiso que nadie puede cumplir, y el Hub lo va a rechazar. Pero el pack
/// funciona: los assets están, el índice cierra, el juego corre. Bloquearlo acá
/// sería castigar al jugador por un problema legal de otro.
///
/// Lo que sí impide jugar es lo que hace que el pack no ANDE en esta máquina:
/// que sea de otro juego, que pida un motor más nuevo, que su formato no se
/// entienda, o que esté roto.
pub(crate) fn blocks_play(code: &str) -> bool {
    // UNA sola implementación del criterio (#530): la misma que usa el grado de
    // compatibilidad del core, y por lo tanto la misma que van a contestar el
    // SDK y el Hub. Tres respuestas distintas a «¿este pack corre acá?» es
    // exactamente lo que la matriz de cuatro grados existe para no tener.
    //
    // Sigue siendo allowlist y no blacklist, sólo que ahora la allowlist vive
    // en un lugar: un código nuevo del validador NO bloquea el juego hasta que
    // alguien decida que debe hacerlo. Al revés —bloquear por defecto— cada
    // chequeo nuevo de autoría le rompería la biblioteca a los jugadores sin
    // que nadie lo pidiera. Por eso `provenance.missing`, que es error, no
    // impide jugar: habla de publicar, no de correr.
    ayther_core::pack_validate::affects_compatibility(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    const MANIFEST: &str = r#"
[pack]
name         = "Golden Axe HD"
version      = "2.1.0"
game_id      = "goldenaxe"
author       = "Ana"
contributors = ["Beto", "Cami"]
description  = "Remasterización completa"
ayther_min   = "0.1.0"
schema       = 2
license      = "CC-BY-4.0"

[regions]
default   = "NTSC"
supported = ["NTSC"]

[systems]
included = ["sprites", "tiles", "music"]

[[profile]]
id = "faithful"
name = "Fiel"
default = true
"#;

    /// Escribe un `.ay`. Con `indice`, además le hornea el `integrity.toml` que
    /// cierra — que es lo único que vuelve auditable a un pack en CE.
    fn write(dir: &Path, name: &str, entries: &[(&str, &[u8])], index: bool) -> std::path::PathBuf {
        let path = dir.join(name);
        let f = std::fs::File::create(&path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut idx = String::new();
        for (n, d) in entries {
            z.start_file(*n, opts).unwrap();
            z.write_all(d).unwrap();
            idx.push_str(&format!(
                "[[entry]]\npath = \"{n}\"\nsha256 = \"{:x}\"\nsize = {}\n",
                Sha256::digest(d),
                d.len()
            ));
        }
        if index {
            z.start_file("integrity.toml", opts).unwrap();
            z.write_all(idx.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        path
    }

    /// Lo DECLARADO sale del manifest, completo: es la mitad de la ficha.
    #[test]
    fn the_card_shows_what_the_manifest_declares() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "ga.ay",
            &[
                ("manifest.toml", MANIFEST.as_bytes()),
                ("assets/a.png", &[1, 2, 3]),
                (
                    "credits.toml",
                    b"[[credit]]\nauthor = \"Ana\"\nrole = \"Arte\"\nassets = 9\n",
                ),
            ],
            true,
        );
        let d = read(&p, None);

        assert_eq!(d.name, "Golden Axe HD");
        assert_eq!(d.version, "2.1.0");
        assert_eq!(d.author, "Ana");
        assert_eq!(d.contributors, vec!["Beto", "Cami"]);
        assert_eq!(d.license, "CC-BY-4.0");
        assert!(d.systems.contains(&"sprites".to_string()));
        assert!(d.systems.contains(&"music".to_string()));
        assert!(!d.systems.contains(&"shaders".to_string()));
        assert!(d.profiles.iter().any(|(id, _)| id == "faithful"));
        assert_eq!(d.credits.len(), 1, "los créditos de #314 entran a la ficha");
        assert!(d.size_bytes > 0);
        assert_eq!(d.audit, Audit::Complete);
    }

    /// EL RIESGO QUE ANOTA LA ISSUE, en su forma de CE: la ficha no puede
    /// mostrar como comprobado lo que no comprobó. Un pack sin índice se juega
    /// igual —son los packs viejos, de antes de #332— pero lo DICE.
    #[test]
    fn without_an_index_the_card_does_not_claim_the_pack_is_whole() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "viejo.ay",
            &[("manifest.toml", MANIFEST.as_bytes())],
            false,
        );
        let d = read(&p, None);

        assert_eq!(d.audit, Audit::MissingIndex);
        assert!(d.audit.word().contains("no se puede auditar"));
        assert!(
            d.playable(),
            "un pack sin índice se puede jugar; hallazgos: {:?}",
            d.findings
        );
    }

    /// EL ORÁCULO DE LA COLUMNA VACÍA. La ficha de CE no tiene un campo de
    /// procedencia, y este test se cae si alguien le agrega uno: el resumen no
    /// puede hablar de firmas cuando este binario no tiene con qué juzgarlas.
    #[test]
    fn the_card_says_nothing_about_the_pack_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "ga.ay",
            &[("manifest.toml", MANIFEST.as_bytes())],
            true,
        );
        let l = read(&p, None).compat_label().to_lowercase();
        assert!(
            !l.contains("firm") && !l.contains("autén") && !l.contains("confia"),
            "«{l}» le promete al jugador algo que CE no puede saber (ADR-005)"
        );
    }

    /// Un pack al que le falta un archivo que su índice declara no se ofrece.
    /// Es la única alarma que CE puede levantar sobre un `.ay`, y la levanta.
    #[test]
    fn a_pack_that_does_not_check_out_is_not_playable() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("roto.ay");
        {
            let f = std::fs::File::create(&p).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let o: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            z.start_file("manifest.toml", o).unwrap();
            z.write_all(MANIFEST.as_bytes()).unwrap();
            // El índice declara un asset que NO está en el archivo.
            z.start_file("integrity.toml", o).unwrap();
            z.write_all(
                format!(
                    "[[entry]]\npath = \"manifest.toml\"\nsha256 = \"{:x}\"\nsize = {}\n\
                 [[entry]]\npath = \"assets/a.png\"\nsha256 = \"00\"\nsize = 3\n",
                    Sha256::digest(MANIFEST.as_bytes()),
                    MANIFEST.len()
                )
                .as_bytes(),
            )
            .unwrap();
            z.finish().unwrap();
        }
        let d = read(&p, None);

        assert!(matches!(d.audit, Audit::Mismatch(_)), "{:?}", d.audit);
        assert!(
            !d.playable(),
            "con el archivo incompleto no se ofrece jugarlo"
        );
        assert!(d.compat_label().contains("No se puede usar"));
    }

    /// Los incompatibles se identifican ANTES de iniciar: con la ROM del
    /// jugador, el validador contesta si el pack es para ESE juego.
    #[test]
    fn a_pack_for_another_game_is_flagged_before_starting() {
        let dir = tempfile::tempdir().unwrap();
        let man = MANIFEST.to_string() + "\n[compat]\nrom_crc32 = \"deadbeef\"\n";
        let p = write(
            dir.path(),
            "otro.ay",
            &[("manifest.toml", man.as_bytes())],
            true,
        );

        // La ROM del jugador NO es la que el pack declara.
        let d = read(&p, Some("12345678"));
        assert!(d.incompatible, "hallazgos: {:?}", d.findings);
        assert!(!d.playable());
        assert!(
            d.findings.iter().any(|f| f.code == "rom.mismatch"),
            "{:?}",
            d.findings
        );

        // Y con la ROM correcta, el mismo pack pasa: el hallazgo es de la
        // comparación, no del pack.
        let d2 = read(&p, Some("deadbeef"));
        assert!(
            !d2.findings.iter().any(|f| f.code == "rom.mismatch"),
            "{:?}",
            d2.findings
        );
    }

    /// Sin ROM conocida no se afirma compatibilidad: «no se comparó» y
    /// «coincide» son cosas distintas, y confundirlas es el riesgo de la issue
    /// en su otra forma.
    #[test]
    fn without_a_known_rom_no_compatibility_is_claimed() {
        let dir = tempfile::tempdir().unwrap();
        let man = MANIFEST.to_string() + "\n[compat]\nrom_crc32 = \"deadbeef\"\n";
        let p = write(
            dir.path(),
            "sinrom.ay",
            &[("manifest.toml", man.as_bytes())],
            true,
        );

        let d = read(&p, None);
        assert!(
            !d.findings.iter().any(|f| f.code == "rom.mismatch"),
            "sin ROM no puede haber mismatch"
        );
        assert!(
            d.findings.iter().any(|f| f.code.starts_with("rom.")),
            "pero SÍ tiene que decir que no se comparó: {:?}",
            d.findings
        );
    }

    /// `[systems]` ausente no es `[systems]` vacío (#294). Sin la distinción, la
    /// ficha diría «no trae nada» sobre un pack viejo que trae de todo.
    #[test]
    fn systems_missing_is_not_systems_empty() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_without_systems = MANIFEST.replace(
            "[systems]\nincluded = [\"sprites\", \"tiles\", \"music\"]\n",
            "",
        );
        let p = write(
            dir.path(),
            "viejo.ay",
            &[("manifest.toml", manifest_without_systems.as_bytes())],
            true,
        );
        let d = read(&p, None);
        assert!(!d.systems_declared, "el pack no declara systems");
        assert!(d.systems.is_empty());

        let p2 = write(
            dir.path(),
            "nuevo.ay",
            &[("manifest.toml", MANIFEST.as_bytes())],
            true,
        );
        let d2 = read(&p2, None);
        assert!(d2.systems_declared);
        assert_eq!(d2.systems.len(), 3);
    }

    /// LA DISTINCIÓN QUE MÁS IMPORTA, y la destapó un test que fallaba: no todo
    /// error del validador impide JUGAR.
    ///
    /// `provenance.missing` —una licencia que exige atribución sin
    /// `credits.toml`— es grave para el AUTOR: afirma un permiso que nadie puede
    /// cumplir y el Hub lo va a rechazar. Pero el pack funciona. Bloquearlo acá
    /// sería castigar al jugador por un problema legal de otro.
    #[test]
    fn a_publishing_error_does_not_prevent_playing() {
        let dir = tempfile::tempdir().unwrap();
        // CC-BY exige atribución y no hay credits.toml: el validador da ERROR.
        let p = write(
            dir.path(),
            "sincreditos.ay",
            &[
                ("manifest.toml", MANIFEST.as_bytes()),
                ("assets/a.png", &[1, 2, 3]),
            ],
            true,
        );
        let d = read(&p, None);

        assert!(
            d.findings
                .iter()
                .any(|f| f.code == "provenance.missing" && f.level == "error"),
            "el validador lo marca como error: {:?}",
            d.findings
        );
        assert!(!d.incompatible, "…pero NO impide jugarlo");
        assert!(d.playable(), "el jugador puede jugarlo igual");
    }

    /// Y el otro lado: lo que sí impide jugar, impide jugar. Sin este control,
    /// la regla de arriba podría estar dejando pasar todo.
    #[test]
    fn an_execution_error_does_prevent_playing() {
        assert!(blocks_play("rom.mismatch"), "otro juego");
        assert!(blocks_play("engine.too_old"), "pide un motor más nuevo");
        assert!(blocks_play("schema.newer"), "formato que no entendemos");
        assert!(blocks_play("manifest.missing"));

        assert!(
            !blocks_play("provenance.missing"),
            "es del autor, no del jugador"
        );
        assert!(!blocks_play("meta.license"));
        assert!(!blocks_play("assets.empty"));
        // Un código NUEVO no bloquea hasta que alguien lo decida: es allowlist.
        // Al revés, cada chequeo de autoría que se agregue le rompería la
        // biblioteca a los jugadores sin que nadie lo pidiera.
        assert!(!blocks_play("codigo.que.no.existe.todavia"));
    }

    /// Un archivo que no es un pack no revienta la pantalla: la ficha se arma
    /// igual y el validador dice por qué. Una pantalla vacía sin motivo es peor
    /// que una con el motivo.
    #[test]
    fn a_broken_file_does_not_blow_up_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("basura.ay");
        std::fs::write(&p, b"no soy un pack").unwrap();

        let d = read(&p, None);
        assert_eq!(d.name, "basura", "cae al nombre del archivo");
        assert!(!d.playable());
        assert!(!d.findings.is_empty(), "y dice por qué");
    }
}
