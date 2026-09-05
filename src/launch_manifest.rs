//! launch_manifest.rs — el manifest inmutable de UNA sesión (#596).
//!
//! # Qué es
//!
//! Todo lo que define una partida, en un archivo: la ROM y su hash, el core, el
//! pack, el perfil, los subsistemas, el audio, el video y la versión de Play que
//! lo escribió. Se guarda en `<datos>/sessions/<id>/launch.toml` **antes** de
//! lanzar, y de ahí salen los argumentos del runtime.
//!
//! # Por qué un archivo y no argumentos sueltos
//!
//! Hasta ahora la partida existía sólo mientras corría: los args se armaban en
//! memoria, se pasaban al proceso y se perdían. Eso deja tres cosas imposibles.
//!
//! **Reproducir.** «Se me colgó» no se puede investigar sin saber con qué corrió.
//! Con el archivo, copiarlo es reproducir la sesión exacta — el CA de la issue.
//!
//! **El modo seguro (#601).** Después de un crash hay que volver a arrancar
//! *sin* lo que pudo haberlo causado. Para eso hace falta saber qué había, y una
//! línea de comandos que ya se ejecutó no lo dice.
//!
//! **Cerrar la sesión.** Un manifest por sesión, con su id, es lo que permite
//! detectar sesiones huérfanas: la carpeta quedó, el proceso no.
//!
//! # Inmutable
//!
//! Se escribe una vez y no se toca. Si el jugador cambia algo, es otra sesión y
//! otro archivo. Un manifest que se edita mientras corre no describe ninguna
//! partida en particular: describe la última vez que alguien lo miró.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::launch_config::LaunchConfig;

/// La versión del FORMATO del manifest.
///
/// Sube cuando cambia el significado de un campo, no cuando se agrega uno: los
/// campos nuevos entran con `serde(default)` y un manifest viejo sigue leyéndose.
/// Existe para que el modo seguro (#601) pueda negarse a reproducir un manifest
/// que no entiende, en vez de arrancar una sesión con la mitad de las opciones.
pub const FORMAT: u32 = 1;

/// La sesión, entera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchManifest {
    #[serde(default = "default_format")]
    #[serde(alias = "formato")]
    pub format: u32,
    /// Id de la sesión: el nombre de su carpeta.
    pub id: String,
    /// Cuándo se creó (UTC ISO-8601). Es del manifest y no del sistema de
    /// archivos porque copiar el archivo tiene que conservar CUÁNDO fue esa
    /// partida — que es medio punto de poder reproducirla.
    #[serde(alias = "creado")]
    pub created: String,
    /// Quién lo escribió. Un manifest de otra versión de Play puede tener
    /// defaults distintos, y saberlo es la diferencia entre reproducir y
    /// aproximar.
    pub play_version: String,

    // -- Lo que se corre ------------------------------------------------------
    pub rom: String,
    /// CRC32 de la ROM, en hexadecimal. **Es lo que hace verificable al
    /// manifest**: la ruta puede apuntar a otro archivo mañana, el hash no.
    #[serde(default)]
    pub rom_crc32: String,
    pub core: String,
    /// Vacío = se juega el original.
    #[serde(default)]
    pub pack: String,

    // -- Lo que el jugador eligió --------------------------------------------
    #[serde(default)]
    pub profile: String,
    /// `None` = no tocó nada y manda el perfil. Se conserva la distinción —en
    /// vez de resolverla al escribir— porque resolverla convertiría «no elegí»
    /// en «elegí esto», y el pack siguiente ya no podría cambiar su perfil.
    #[serde(default)]
    pub subsystems: Option<u32>,
    #[serde(default)]
    pub muted_buses: u32,
    #[serde(default = "default_true")]
    pub shaders: bool,
    #[serde(default)]
    pub output: String,
    #[serde(default)]
    pub no_flicker: bool,
    #[serde(default)]
    pub patch: String,

    /// #604 — el guardado desde el que se reanuda. Vacío = partida limpia.
    ///
    /// Va en el manifest y no en `LaunchConfig` porque no es una preferencia:
    /// es una decisión de ESTA partida. Guardarlo con la configuración haría
    /// que «Jugar» de cero quedara recordado como «reanudar desde aquel
    /// guardado», y la próxima vez que alguien apretara Jugar reanudaría.
    #[serde(default)]
    #[serde(alias = "guardado")]
    pub save: String,
}

fn default_format() -> u32 {
    FORMAT
}
fn default_true() -> bool {
    true
}

impl LaunchManifest {
    /// Arma el manifest de una partida.
    ///
    /// `id` viaja por parámetro y no se genera acá para que el test pueda fijar
    /// uno: un id aleatorio adentro haría que el round-trip no se pudiera
    /// comparar sin ignorar campos, y un round-trip con excepciones no prueba
    /// que el archivo describa la sesión.
    pub fn new(
        id: &str,
        rom: &Path,
        rom_crc32: &str,
        core: &Path,
        pack: Option<&Path>,
        cfg: &LaunchConfig,
    ) -> Self {
        LaunchManifest {
            format: FORMAT,
            id: id.to_string(),
            created: crate::date::now_utc(),
            play_version: env!("CARGO_PKG_VERSION").to_string(),
            rom: rom.to_string_lossy().into_owned(),
            rom_crc32: rom_crc32.to_string(),
            core: core.to_string_lossy().into_owned(),
            pack: pack
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            profile: cfg.profile.clone(),
            subsystems: cfg.subsystems,
            muted_buses: cfg.muted_buses,
            shaders: cfg.shaders,
            output: cfg.output.clone(),
            no_flicker: cfg.no_flicker,
            patch: cfg.patch.clone(),
            save: String::new(),
        }
    }

    /// La parte del manifest que es una elección del jugador, de vuelta como
    /// `LaunchConfig`.
    ///
    /// Existe porque `to_args` deriva del manifest y no de la config, pero el
    /// derivador vive en `launch_config` — duplicarlo sería tener dos lugares
    /// que traducen las mismas opciones a la misma línea de comandos, y ahí es
    /// donde una se queda atrás.
    pub fn config(&self) -> LaunchConfig {
        LaunchConfig {
            profile: self.profile.clone(),
            subsystems: self.subsystems,
            muted_buses: self.muted_buses,
            shaders: self.shaders,
            output: self.output.clone(),
            no_flicker: self.no_flicker,
            patch: self.patch.clone(),
        }
    }

    /// Los argumentos del runtime, DERIVADOS del manifest.
    ///
    /// Es el único camino: antes cada llamador armaba los suyos, y un argumento
    /// que se agregara en un lado no aparecía en el otro. Ahora el manifest es
    /// la fuente y los args son su proyección — si algo no está en el archivo,
    /// no llega al runtime, que es lo que hace cierto «reproducir copiando el
    /// archivo».
    /// Todavía la llama sólo su oráculo —quien lanza hoy arma los args desde la
    /// `LaunchConfig`—, y el `allow` no cubre `cargo test`: si el oráculo se va, el aviso vuelve.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn to_args(&self) -> Vec<String> {
        let mut a = self.config().to_args();
        // #604: reanudar es parte de la sesión, no de la configuración, así que
        // se agrega acá y no en `LaunchConfig::to_args`. Es además lo que hace
        // que un manifest copiado reanude exactamente igual: la ruta del
        // guardado viaja con él.
        if !self.save.is_empty() {
            a.push("--load-state".into());
            a.push(self.save.clone());
        }
        a
    }

    /// ¿Este build entiende el manifest? Un formato MAYOR se rechaza — misma
    /// regla que el `schema` del pack, y por el mismo motivo: arrancar con la
    /// mitad de las opciones sería peor que no arrancar.
    pub fn supported(&self) -> bool {
        self.format <= FORMAT
    }
}

/// La carpeta de una sesión: `<datos>/sessions/<id>/`.
pub fn session_dir(data: &Path, id: &str) -> PathBuf {
    data.join("sessions").join(id)
}

/// Escribe el manifest y devuelve su ruta.
///
/// **Atómico**, por lo mismo que la caché de la biblioteca (#591): un manifest
/// truncado no parsea, y un manifest que no parsea es una sesión que no se puede
/// reproducir ni cerrar — justo cuando más falta hace, que es después de un
/// crash.
pub fn write(data: &Path, m: &LaunchManifest) -> std::io::Result<PathBuf> {
    let dir = session_dir(data, &m.id);
    std::fs::create_dir_all(&dir)?;
    let destination = dir.join("launch.toml");
    let tmp = dir.join("launch.toml.tmp");
    let text = toml::to_string_pretty(m)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &destination)?;
    Ok(destination)
}

/// Lee un manifest. `None` si no está o no se entiende — las dos cosas
/// significan lo mismo para quien lo iba a reproducir.
pub fn read(path: &Path) -> Option<LaunchManifest> {
    let text = std::fs::read_to_string(path).ok()?;
    let m: LaunchManifest = toml::from_str(&text).ok()?;
    m.supported().then_some(m)
}

/// Un id de sesión: la fecha compacta más un sufijo del reloj.
///
/// Legible a propósito —`20260827T164512-3f2a`— porque el id es el nombre de una
/// carpeta que alguien va a mirar cuando algo salga mal, y un UUID no dice
/// cuándo fue esa partida. El sufijo evita la colisión de dos lanzamientos en el
/// mismo segundo.
pub fn new_id() -> String {
    let timestamp = crate::date::now_utc();
    let compact_timestamp: String = timestamp
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let nanoseconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{compact_timestamp}-{:04x}", nanoseconds & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LaunchConfig {
        LaunchConfig {
            profile: "faithful".into(),
            subsystems: Some(0b1011),
            muted_buses: 0b10,
            shaders: false,
            output: "crt".into(),
            no_flicker: true,
            patch: "D:/parches/es.ips".into(),
        }
    }

    fn manifest() -> LaunchManifest {
        LaunchManifest::new(
            "20260827T164512-3f2a",
            Path::new("D:/roms/Golden Axe.md"),
            "665D7DF9",
            Path::new("C:/cores/gpgx.dll"),
            Some(Path::new("D:/roms/Golden Axe.ay")),
            &cfg(),
        )
    }

    /// EL CA: lo que se escribe es lo que se lee, campo por campo.
    ///
    /// Sin excepciones ni campos ignorados: un round-trip que tolera diferencias
    /// no prueba que el archivo describa la sesión, sólo que se parece.
    #[test]
    fn the_manifest_round_trips_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest();
        let path = write(dir.path(), &m).unwrap();

        let read_back = read(&path).expect("el manifest recién escrito tiene que leerse");
        assert_eq!(read_back, m, "el round-trip perdió o cambió algo");
    }

    /// Un manifest = UNA sesión: la ruta sale del id, y dos ids distintos no
    /// comparten archivo. Si compartieran, reproducir una sesión traería la
    /// configuración de otra.
    #[test]
    fn each_session_has_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = manifest();
        let mut b = manifest();
        b.id = "20260827T170000-beef".into();
        b.profile = "enhanced".into();

        let ra = write(dir.path(), &a).unwrap();
        let rb = write(dir.path(), &b).unwrap();
        assert_ne!(ra, rb, "dos sesiones, dos archivos");
        assert_eq!(read(&ra).unwrap().profile, "faithful");
        assert_eq!(read(&rb).unwrap().profile, "enhanced");

        // Y re-escribir el mismo id lo pisa: es LA sesión, no un historial.
        a.profile = "otro".into();
        write(dir.path(), &a).unwrap();
        assert_eq!(read(&ra).unwrap().profile, "otro");
    }

    /// Reproducir una sesión es copiar el archivo: los argumentos del runtime
    /// salen de él y de nada más. Si algo de la partida no está en el manifest,
    /// la copia arranca distinto — y ahí «reproducir» sería una promesa falsa.
    #[test]
    fn the_args_come_from_the_manifest_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest();
        let path = write(dir.path(), &m).unwrap();

        // La copia, leída en otra máquina, da los MISMOS argumentos.
        let copy = read(&path).unwrap();
        assert_eq!(copy.to_args(), m.to_args());

        // Y son los que la config produce: el derivador es uno solo.
        assert_eq!(m.to_args(), cfg().to_args());

        // Con lo que el jugador eligió adentro, no con defaults.
        let a = m.to_args().join(" ");
        for expected in [
            "--profile faithful",
            "--subsystems 11",
            "--mute-buses 2",
            "--no-shaders",
            "--output crt",
            "--core-option",
            "--patch D:/parches/es.ips",
        ] {
            assert!(a.contains(expected), "falta «{expected}» en: {a}");
        }
    }

    /// `subsystems: None` NO es lo mismo que `Some(0)`, y el archivo tiene que
    /// conservar la diferencia: una es «manda el perfil» y la otra «apagá todo».
    /// Resolverla al escribir convertiría «no elegí» en una elección, y el pack
    /// siguiente ya no podría cambiar su perfil.
    #[test]
    fn not_having_chosen_survives_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = manifest();
        m.subsystems = None;
        let path = write(dir.path(), &m).unwrap();
        let read_back = read(&path).unwrap();

        assert_eq!(read_back.subsystems, None);
        assert!(
            !read_back.to_args().join(" ").contains("--subsystems"),
            "sin elección no se manda la máscara"
        );

        m.subsystems = Some(0);
        let updated_path = write(dir.path(), &m).unwrap();
        assert_eq!(read(&updated_path).unwrap().subsystems, Some(0));
        assert!(
            read(&updated_path)
                .unwrap()
                .to_args()
                .join(" ")
                .contains("--subsystems 0"),
            "apagar todo SÍ se manda"
        );
    }

    /// Un formato más nuevo se rechaza en vez de arrancar a medias. Es la misma
    /// regla que el `schema` del pack: el modo seguro (#601) va a reproducir
    /// manifests, y reproducir uno que no se entiende es lanzar una partida
    /// distinta de la que se quería investigar.
    #[test]
    fn a_newer_format_does_not_play() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = manifest();
        m.format = FORMAT + 1;
        let path = write(dir.path(), &m).unwrap();
        assert!(read(&path).is_none(), "un formato del futuro no se lee");

        m.format = FORMAT;
        let path = write(dir.path(), &m).unwrap();
        assert!(read(&path).is_some());
    }

    /// Un manifest sin los campos opcionales —uno viejo, o escrito a mano— se
    /// lee igual. Es lo que permite que el formato crezca sin invalidar lo ya
    /// guardado.
    #[test]
    fn a_minimal_manifest_reads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("launch.toml");
        std::fs::write(
            &p,
            "id = \"x\"\ncreado = \"2026-08-27T00:00:00Z\"\n\
                            play_version = \"0.1.0\"\nrom = \"a.md\"\n\
                            core = \"c.dll\"\n",
        )
        .unwrap();
        let m = read(&p).expect("un manifest mínimo tiene que leerse");
        assert_eq!(m.format, FORMAT, "sin formato declarado, es el de hoy");
        assert!(m.shaders, "el default de shaders es encendido");
        assert_eq!(m.subsystems, None);
        assert!(m.pack.is_empty(), "sin pack se juega el original");
    }

    /// Los ids no colisionan entre lanzamientos seguidos, que es lo único que
    /// tienen que garantizar: si colisionaran, la segunda partida pisaría el
    /// manifest de la primera y quedaría una sesión sin registro.
    #[test]
    fn two_back_to_back_launches_do_not_share_an_id() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        // Y son nombres de carpeta válidos: el id se usa como ruta.
        for id in [&a, &b] {
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "«{id}» no sirve como nombre de carpeta"
            );
        }
    }

    /// #604 — EL CA: Continuar reanuda EXACTAMENTE el último guardado.
    ///
    /// La ruta viaja en el manifest y de ahí a los argumentos, así que copiar
    /// el archivo reanuda la misma partida y no el mismo juego desde el
    /// principio — que es la diferencia entre reproducir una sesión y repetir
    /// un lanzamiento.
    #[test]
    fn continue_resumes_from_the_manifest_save() {
        let mut m = test_manifest();
        m.save = "D:/saves/golden/hd/estado-20260827T210000Z-a1.bin".into();
        let a = m.to_args();
        let i = a
            .iter()
            .position(|x| x == "--load-state")
            .expect("va la bandera");
        assert_eq!(
            a[i + 1],
            "D:/saves/golden/hd/estado-20260827T210000Z-a1.bin"
        );
    }

    /// Y EL OTRO LADO: Jugar no lo pisa. Sin `guardado`, no se manda nada — el
    /// Runtime arranca de cero y, por el layout de #603, su salida escribe una
    /// versión NUEVA. Una partida limpia no puede borrar la anterior aunque el
    /// jugador la abandone en el primer nivel.
    #[test]
    fn playing_clean_sends_no_save() {
        let m = test_manifest();
        assert_eq!(m.save, "", "una sesion nueva no reanuda por defecto");
        assert!(
            !m.to_args().iter().any(|x| x == "--load-state"),
            "sin guardado no se manda la bandera: {:?}",
            m.to_args()
        );
    }

    /// El campo sobrevive al round-trip por disco: un manifest de una sesión
    /// que reanudó tiene que poder reproducirla mañana.
    #[test]
    fn the_save_survives_the_round_trip() {
        let mut m = test_manifest();
        m.save = "D:/saves/g/hd/estado-20260827T210000Z-a1.bin".into();
        let text = toml::to_string_pretty(&m).unwrap();
        let read_back: LaunchManifest = toml::from_str(&text).unwrap();
        assert_eq!(read_back, m);
    }

    /// Y un manifest viejo —de antes de #604— se sigue leyendo: el campo entra
    /// con `default`, así que no reanuda, que es lo que hacía.
    #[test]
    fn a_manifest_from_before_604_reads_and_does_not_resume() {
        let previous = r#"
id = "s1"
creado = "2026-08-27T20:00:00Z"
play_version = "0.1.0"
rom = "D:/ROMs/g.md"
core = "D:/cores/gpgx.dll"
"#;
        let m: LaunchManifest = toml::from_str(previous).expect("un manifest viejo dejo de leerse");
        assert_eq!(m.save, "");
        assert!(!m.to_args().iter().any(|x| x == "--load-state"));
    }

    /// Un helper propio para estos cuatro: el resto del módulo arma manifests
    /// con `nuevo()`, que ya prueba lo suyo.
    fn test_manifest() -> LaunchManifest {
        LaunchManifest::new(
            "s1",
            Path::new("D:/ROMs/g.md"),
            "a1b2c3d4",
            Path::new("D:/cores/gpgx.dll"),
            None,
            &LaunchConfig::default(),
        )
    }
}
