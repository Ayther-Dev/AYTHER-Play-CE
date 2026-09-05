//! pack_meta.rs — lo que el launcher necesita saber de un `.ay`.
//!
//! Tres cosas: la identidad para el chequeo de ROM equivocada, las capacidades
//! que la pantalla previa ofrece (#311) y los créditos (#314).
//!
//! **Todo sale del lector del core** (`ayther_core::archive_vfs`), que es el
//! mismo que usa el motor. Hasta #312 este archivo re-implementaba el formato
//! con `zip` + `toml` —abría el ZIP, buscaba el manifest, parseaba a mano— y
//! funcionaba; ése era el problema. Dos lectores del mismo formato se separan
//! el día que uno arregla un borde y el otro no, y el borde que se separa es el
//! que decide si un pack se acepta: la pantalla ofrecía un perfil que la sesión
//! no aplicaba, o daba por bueno un pack que el motor no abre.
//!
//! La ficha completa —autoría, firma, compatibilidad, hallazgos— vive en
//! `pack_details.rs`.

use std::path::Path;

/// Metadata extracted from a `.ay` pack's `manifest.toml`.
#[derive(Debug, Clone)]
pub struct PackInfo {
    /// Symbolic game identifier set by the pack author (e.g. `"sonic2"`).
    ///
    /// Nadie lo lee todavía —la compatibilidad la decide el validador del core
    /// (#530), no este campo—, pero se sigue leyendo del manifest porque es
    /// parte del formato y quien mire este struct tiene que ver qué trae.
    #[allow(dead_code)]
    pub game_id: String,
    /// Human-readable pack name (e.g. `"Sonic 2 HD"`).
    pub name: String,
    /// #602 — la versión declarada en el manifest.
    ///
    /// **Es el CA de la issue**: la opinión del jugador es por PACK Y VERSIÓN,
    /// y hasta acá Play mandaba `""` — así que una opinión sobre la 1.0 se
    /// leía como si hablara de cualquier versión, y quien arreglaba su pack no
    /// volvía a saber si había mejorado.
    pub version: String,
}

/// Pack health as seen by the launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PackStatus {
    /// No `.ay` pack found next to this ROM.
    None = 0,
    /// Pack found and manifest is readable.  game_id matches or no DAT to check.
    Ok = 1,
    /// Pack found but manifest could not be read (corrupt / old format).
    Corrupt = 2,
}

/// Try to open a `.ay` pack and read its manifest.
///
/// Returns `(PackStatus, Option<PackInfo>)`.
/// Never panics — all errors are silently converted to `Corrupt`.
pub fn read(pack_path: &Path) -> (PackStatus, Option<PackInfo>) {
    if !pack_path.exists() {
        return (PackStatus::None, None);
    }

    match read_inner(pack_path) {
        Ok(info) => (PackStatus::Ok, Some(info)),
        Err(_) => (PackStatus::Corrupt, None),
    }
}

/// #312: el manifest sale del LECTOR DEL CORE, no de un parser propio.
///
/// Esta función re-implementaba el formato con `zip` + `toml`: abría el ZIP,
/// buscaba `manifest.toml` y leía dos campos a mano. Funcionaba, y ése era el
/// problema — dos lectores del mismo formato se separan el día que uno arregla
/// un borde y el otro no, y el borde que se separa es el que decide si un pack
/// se acepta. Ahora es el mismo `AyArchive` que usa el motor.
fn read_inner(pack_path: &Path) -> Result<PackInfo, Box<dyn std::error::Error>> {
    let path = pack_path.to_string_lossy().to_string();
    let pack =
        ayther_core::archive_vfs::AyArchive::open_verbose(&path).map_err(|e| format!("{e:?}"))?;
    Ok(PackInfo {
        game_id: pack.meta.game_id.clone(),
        name: pack.meta.name.clone(),
        version: pack.meta.version.clone(),
    })
}

/// #311: lo que este pack PUEDE ofrecer, leido del mismo `manifest.toml`.
///
/// Se lee del manifest y no de una lista fija: es lo que evita ofrecer «Musica»
/// en un pack sin audio — el riesgo que anota la issue. Un pack ilegible
/// devuelve capacidades vacias, y ahi la pantalla ofrece lo minimo en vez de
/// inventar.
pub fn capabilities(pack_path: &Path) -> crate::launch_config::PackCapabilities {
    use crate::launch_config::PackCapabilities;
    let mut caps = PackCapabilities::default();
    // «original» SIEMPRE esta y va primero: es implicito en el pack (#293) y
    // comparar con el juego sin remasterizar es lo que no puede faltar nunca.
    caps.profiles.push(("original".into(), "Original".into()));

    // El LECTOR DEL CORE, no un parser propio (#312): es el mismo `AyArchive`
    // que usa el motor, así que lo que la pantalla ofrece y lo que la sesión
    // aplica no pueden separarse. Antes esto re-implementaba el formato con
    // `zip` + `toml`, y el día que uno arreglara un borde y el otro no, el
    // jugador vería un perfil que no existe.
    let path = pack_path.to_string_lossy().to_string();
    let Ok(pack) = ayther_core::archive_vfs::AyArchive::open_verbose(&path) else {
        return caps;
    };

    // `[systems]` AUSENTE no es `[systems]` vacío (#294): un pack viejo no
    // declara nada, y ahí lo honesto es ofrecer todo en vez de nada. El lector
    // ya distingue los dos casos — `has_system` devuelve `None` sin declaración.
    const SUBSYSTEMS: [&str; 8] = [
        "sprites",
        "metasprites",
        "tiles",
        "planes",
        "ui",
        "music",
        "sfx",
        "shaders",
    ];
    if pack.has_system("sprites").is_some() {
        caps.declares_systems = true;
        for (i, name) in SUBSYSTEMS.iter().enumerate() {
            if pack.has_system(name) == Some(true) {
                caps.systems |= 1 << i;
            }
        }
    }

    // #296: el perfil de SALIDA que el pack recomienda. Se lee, no se impone:
    // la pantalla lo muestra como «(del pack)» y la elección del jugador gana.
    if let Some(o) = pack.meta.output.as_deref() {
        caps.output_recommended = o.to_string();
    }

    // Los perfiles salen de `profiles()`, que es lo que la SESIÓN va a ver:
    // incluye el «original» implícito y garantiza exactamente un default (#293).
    for p in pack.profiles() {
        if p.id.is_empty() || p.id == "original" {
            continue;
        }
        if caps.profiles.iter().any(|q| q.0 == p.id) {
            continue;
        }
        let name = p.name.clone().unwrap_or_else(|| p.id.clone());
        if p.default {
            caps.default_profile = caps.profiles.len();
        }
        caps.profiles.push((p.id.clone(), name));
    }

    // Un pack sin `[[profile]]` igual ofrece «completo» — el mismo que arma el
    // core (#293). Sin esto, la pantalla mostraria un solo perfil («original»)
    // sobre un pack que si sustituye cosas.
    if caps.profiles.len() == 1 {
        caps.profiles.push(("full".into(), "Completo".into()));
        caps.default_profile = 1;
    } else if caps.default_profile == 0 {
        // El core garantiza exactamente un default; si el manifest no lo dice,
        // gana el primero declarado — no «original», que no sustituye nada.
        caps.default_profile = 1;
    }
    caps
}

// ---------------------------------------------------------------------------
// #314 — los CRÉDITOS del pack: quién lo hizo, qué aportó, bajo qué licencia.
//
// Se leen del `credits.toml` que el pack ya puede llevar (#309) y NO del Hub:
// reconocer al autor no puede depender de estar conectado, y un pack instalado
// a mano tiene los mismos créditos que uno descargado. La lista es lo único de
// la ficha que funciona igual offline y online, y por eso es la primera parte
// de #559 que se puede hacer sin el Hub.
//
// El MODELO viene del core (`ayther_core::pack_credits`), que está escrito
// para este consumidor: acá sólo se saca el archivo del ZIP y se ordena para
// mostrarlo. Re-declarar el formato en Play sería la tercera copia.
// ---------------------------------------------------------------------------

/// Una línea de la pantalla de créditos, ya formateada para la UI.
#[derive(Debug, Clone, PartialEq)]
pub struct CreditLine {
    pub author: String,
    /// Rol declarado ("Arte", "Música"…). Vacío si el pack no lo dice.
    pub role: String,
    /// "12 assets" — lo que esta persona aportó, en unidades del pack.
    pub contribution: String,
    /// Licencias distintas que cubren su aporte, separadas por " · ".
    pub licenses: String,
}

/// Los créditos de un pack, listos para mostrar.
///
/// Orden: por cantidad de assets (descendente) y, a igualdad, alfabético. Que
/// el orden dependa del aporte y no del orden del archivo evita que el autor
/// del pack se ponga primero por escribirse primero — y a igualdad de aporte
/// alfabético es la única regla que no privilegia a nadie.
///
/// Un pack sin `credits.toml`, o con uno ilegible, devuelve la lista vacía: la
/// pantalla dice «este pack no declara créditos», que es la verdad. Nunca se
/// inventa una atribución (mismo criterio que `Credits::parse`).
pub fn credits(pack_path: &Path) -> Vec<CreditLine> {
    let Some(raw) = read_credits_toml(pack_path) else {
        return Vec::new();
    };
    let Some(parsed) = ayther_core::pack_credits::Credits::parse(&raw) else {
        return Vec::new();
    };

    let mut rows: Vec<(u32, CreditLine)> = parsed
        .credits
        .iter()
        .filter(|c| !c.author.trim().is_empty())
        .map(|c| {
            (
                c.assets,
                CreditLine {
                    author: c.author.trim().to_string(),
                    role: c.role.clone().unwrap_or_default().trim().to_string(),
                    contribution: match c.assets {
                        0 => String::new(),
                        1 => "1 asset".to_string(),
                        n => format!("{n} assets"),
                    },
                    licenses: c.licenses.join(" · "),
                },
            )
        })
        .collect();

    rows.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.author.to_lowercase().cmp(&b.1.author.to_lowercase()))
    });
    rows.into_iter().map(|(_, line)| line).collect()
}

/// Los bytes de `credits.toml` dentro del pack, si están.
///
/// Por el lector del core y no por el ZIP crudo (#312): así los créditos pasan
/// por la MISMA verificación por entrada que el resto del contenido. Leerlos
/// del zip sin verificar mostraría una atribución que el índice firmado no
/// respalda — y una atribución que nadie puede sostener es peor que ninguna.
fn read_credits_toml(pack_path: &Path) -> Option<Vec<u8>> {
    let path = pack_path.to_string_lossy().to_string();
    let pack = ayther_core::archive_vfs::AyArchive::open_verbose(&path).ok()?;
    pack.read("credits.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Arma un `.ay` mínimo con las entradas dadas. El pack es un ZIP y lo que
    /// se prueba acá es la lectura, así que se escribe uno de verdad en vez de
    /// mockear el archivo: el defecto que este test tiene que poder ver —una
    /// entrada que no está, o un TOML roto— vive justo en ese borde.
    /// El manifest MÍNIMO QUE EL MOTOR ABRE. Desde que los créditos se leen por
    /// el lector del core (#312), un fixture con dos campos sueltos ya no
    /// alcanza: el pack no abriría y el test probaría el rechazo en vez de la
    /// lectura.
    const MANIFEST_OK: &str = "[pack]
name = \"T\"
version = \"1.0.0\"
                               game_id = \"x\"
ayther_min = \"0.1.0\"
schema = 2

                               [regions]
default = \"NTSC\"
supported = [\"NTSC\"]
";

    fn pack_with(entries: &[(&str, &str)]) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        let mut z = zip::ZipWriter::new(f.reopen().unwrap());
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        // Si el caso no trae manifest, se le pone uno válido: lo que se prueba
        // acá son los CRÉDITOS, y un pack que no abre no llega a esa parte.
        if !entries.iter().any(|(n, _)| *n == "manifest.toml") {
            z.start_file("manifest.toml", opts).unwrap();
            z.write_all(MANIFEST_OK.as_bytes()).unwrap();
        }
        for (name, body) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        f
    }

    const CREDITS: &str = r#"
[[credit]]
author = "Ana"
role = "Arte"
assets = 12
licenses = ["CC-BY-4.0"]

[[credit]]
author = "Beto"
role = "Musica"
assets = 30
licenses = ["CC-BY-4.0", "CC0-1.0"]

[[credit]]
author = "Cami"
assets = 12
"#;

    /// El caso del ciclo: el pack dice quién lo hizo y Play lo muestra, con rol
    /// y aporte, SIN Hub — todo sale del archivo que ya está en el disco.
    #[test]
    fn the_credits_come_from_the_pack_offline() {
        let f = pack_with(&[("credits.toml", CREDITS)]);
        let c = credits(f.path());
        assert_eq!(c.len(), 3);
        // Orden por aporte, no por orden en el archivo: el que más puso, primero.
        assert_eq!(c[0].author, "Beto");
        assert_eq!(c[0].contribution, "30 assets");
        assert_eq!(c[0].role, "Musica");
        assert_eq!(c[0].licenses, "CC-BY-4.0 \u{b7} CC0-1.0");
        // A igualdad de aporte, alfabético — la única regla que no privilegia a
        // nadie, y en particular no al que escribió el archivo.
        assert_eq!(c[1].author, "Ana");
        assert_eq!(c[2].author, "Cami");
        // El rol puede faltar; la línea sigue existiendo.
        assert_eq!(c[2].role, "");
        assert_eq!(c[2].licenses, "");
    }

    /// Un pack sin créditos no es un error: se juega igual y la pantalla dice
    /// que no los declara. Es el control que separa «no hay» de «falló».
    #[test]
    fn a_pack_without_credits_toml_gives_an_empty_list() {
        let f = pack_with(&[("manifest.toml", "[pack]\ngame_id=\"x\"\n")]);
        assert!(credits(f.path()).is_empty());
    }

    /// Un `credits.toml` ilegible tampoco puede inventar una atribución (mismo
    /// criterio que `Credits::parse`, que devuelve None en vez de propagar).
    #[test]
    fn a_broken_credits_does_not_invent_attribution() {
        let f = pack_with(&[("credits.toml", "esto no es TOML [[[")]);
        assert!(credits(f.path()).is_empty());
    }

    /// Un crédito sin autor no ocupa una línea en blanco en la pantalla.
    #[test]
    fn a_credit_without_author_does_not_get_in() {
        let f = pack_with(&[(
            "credits.toml",
            "[[credit]]\nauthor = \"\"\nassets = 5\n\n\
             [[credit]]\nauthor = \"Ana\"\nassets = 1\n",
        )]);
        let c = credits(f.path());
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].author, "Ana");
        assert_eq!(c[0].contribution, "1 asset"); // singular, no "1 assets"
    }

    /// Control de no vacuidad del fixture: si `CREDITS` no tuviera créditos,
    /// los tres tests de arriba pasarían sin probar nada.
    #[test]
    fn the_fixture_has_real_credits() {
        let parsed = ayther_core::pack_credits::Credits::parse(CREDITS.as_bytes());
        assert_eq!(parsed.unwrap().credits.len(), 3);
    }

    // -- #558: el perfil autorado llega de punta a punta --------------------
    //
    // La cadena que la épica pide cerrar es: el autor define el perfil en el
    // Lab · viaja en el pack · Play lo ofrece · el Runtime lo aplica. Cada
    // tramo tenía su prueba —el core sanitiza y garantiza un default, el
    // runtime tiene `--profile`, `launch_config` elige y arma la línea de
    // comandos— y el ÚNICO que no tenía oráculo era justamente el del medio:
    // que lo que Play ofrece salga del pack REAL y no de un fixture en memoria.
    //
    // Ese es el tramo donde la cadena se corta sin que nada avise: el editor
    // autora, el pack lleva, y la pantalla muestra otra cosa.

    const WITH_PROFILES: &str = "\
[pack]
name = \"T\"
version = \"1.0.0\"
game_id = \"x\"
ayther_min = \"0.1.0\"
schema = 2

[regions]
default = \"NTSC\"
supported = [\"NTSC\"]

[[profile]]
id = \"faithful\"
name = \"Fiel\"
systems = [\"sprites\", \"metasprites\"]
muted_buses = []

[[profile]]
id = \"enhanced\"
name = \"Mejorado\"
systems = [\"sprites\", \"metasprites\", \"tiles\", \"planes\", \"music\"]
muted_buses = []
default = true
";

    /// El caso de la épica: dos perfiles autorados llegan a la pantalla con su
    /// id, su nombre y cuál es el predeterminado.
    #[test]
    fn authored_profiles_reach_play() {
        let f = pack_with(&[("manifest.toml", WITH_PROFILES)]);
        let caps = capabilities(f.path());

        // «original» SIEMPRE primero: comparar con el juego sin remasterizar es
        // lo que no puede faltar nunca, y el manifest no lo declara.
        assert_eq!(caps.profiles[0].0, "original");
        assert_eq!(caps.profiles.len(), 3, "original + los dos del pack");
        assert_eq!(caps.profiles[1], ("faithful".into(), "Fiel".into()));
        assert_eq!(caps.profiles[2], ("enhanced".into(), "Mejorado".into()));

        // El default es el del PACK, y por índice — no el primero de la lista.
        assert_eq!(caps.default_profile, 2);
        assert_eq!(caps.profiles[caps.default_profile].0, "enhanced");

        // Y no se cuela el «Completo» sintético: ése es para packs que NO
        // declaran perfiles. Ofrecerlo acá inventaría una opción que el autor
        // no autoró.
        assert!(!caps.profiles.iter().any(|p| p.0 == "full"));
    }

    /// Lo que el jugador elige arranca en el perfil del pack, y la línea de
    /// comandos lo lleva. Es el tramo Play → Runtime, sobre capacidades leídas
    /// del archivo y no fabricadas.
    #[test]
    fn the_pack_profile_travels_to_the_runtime() {
        use crate::launch_config::LaunchConfig;
        let f = pack_with(&[("manifest.toml", WITH_PROFILES)]);
        let caps = capabilities(f.path());

        let cfg = LaunchConfig::recommended(&caps);
        assert_eq!(cfg.profile, "enhanced", "arranca en el default del pack");
        assert!(cfg.to_args().join(" ").contains("--profile enhanced"));

        // Y un perfil que este pack no ofrece no sobrevive al clamp: se cae al
        // recomendado en vez de mandarle al runtime un id que va a rechazar.
        let mut unsupported_profile = LaunchConfig {
            profile: "cinematic".into(),
            ..cfg.clone()
        };
        unsupported_profile.clamp_to(&caps);
        assert_eq!(unsupported_profile.profile, "enhanced");
    }

    /// Un pack que NO declara perfiles sigue ofreciendo dos: «original» y el
    /// «Completo» que arma el core. Es el borde que separa «el autor eligió no
    /// autorar perfiles» de «el pack está roto»: en el primero se juega igual.
    #[test]
    fn a_pack_without_profiles_offers_original_and_full() {
        let f = pack_with(&[]);
        let caps = capabilities(f.path());
        assert_eq!(caps.profiles.len(), 2);
        assert_eq!(caps.profiles[0].0, "original");
        assert_eq!(caps.profiles[1].0, "full");
        assert_eq!(
            caps.default_profile, 1,
            "no arranca en «original»: el pack sustituye"
        );
    }

    /// NO VACUIDAD del fixture. Sin esto, los tres tests de arriba pasarían
    /// igual si `CON_PERFILES` no declarara ningún perfil — «original» y el
    /// «Completo» sintético llenarían la lista y nadie se enteraría de que el
    /// manifest no aportó nada.
    #[test]
    fn the_fixture_declares_real_profiles() {
        let f = pack_with(&[("manifest.toml", WITH_PROFILES)]);
        let pack =
            ayther_core::archive_vfs::AyArchive::open_verbose(f.path().to_string_lossy().as_ref())
                .unwrap();
        let ps = pack.profiles();
        assert!(ps.iter().any(|p| p.id == "faithful"));
        assert!(ps.iter().any(|p| p.id == "enhanced"));
        // Y las máscaras son DISTINTAS: dos perfiles que filtran lo mismo no
        // demuestran que el filtrado llegue, aunque los nombres difieran.
        let a = ps
            .iter()
            .find(|p| p.id == "faithful")
            .unwrap()
            .systems_mask();
        let b = ps
            .iter()
            .find(|p| p.id == "enhanced")
            .unwrap()
            .systems_mask();
        assert_ne!(a, b);
        assert_ne!(a, 0);
    }
}
