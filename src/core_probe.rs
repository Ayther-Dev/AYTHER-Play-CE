//! core_probe.rs — validar el core que trajo el jugador (#589, PLAY-EP01.2).
//!
//! Play es BYOC: el core lo pone el jugador. Y Play **no lo carga nunca**.
//! Cargar un core libretro es meter código GPL en el proceso del launcher, que
//! es justo la frontera que el ecosistema no cruza (spec de producto, §7); y
//! además una DLL ajena que crashea al cargarse se lleva puesta la aplicación,
//! que en una TV significa volver al escritorio sin explicación.
//!
//! Así que la validación se hace en dos pasos, y ninguno carga nada acá:
//!
//! 1. **El header PE**, leyendo 64 bytes del archivo. Distingue x86 de x64
//!    *antes* de gastar un proceso, y es el único que puede: para Windows un
//!    core de 32 bits y un archivo inexistente son el mismo error
//!    (`LoadLibrary` devuelve 193 y 126, el runtime informa `no_carga` en los
//!    dos casos). Sin este paso, el jugador que bajó la build equivocada lee
//!    «no se pudo cargar» y no tiene qué hacer con eso.
//! 2. **El sondeo fuera de proceso**: `ayther_runtime --probe-core <dll>`, que
//!    sí lo carga, dice qué es y se muere. Play lee su línea `AYTHER_STATUS`.
//!
//! Lo que sale es un `Veredicto` con un mensaje que dice **qué hacer**, no qué
//! falló. «No se pudo cargar el core» no es un mensaje: es el registro de que
//! nadie pensó en el caso.

use crate::tr;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Arquitectura, leída del header sin cargar el archivo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    X64,
    X86,
    Arm64,
    /// Un PE válido de una máquina que no conocemos (el valor es el campo
    /// `Machine` crudo, para que el reporte no lo pierda).
    Other(u16),
    /// No es un PE. En Windows significa «esto no es una DLL»; en Linux y macOS
    /// significa que este chequeo no aplica y decide el sondeo (ELF y Mach-O
    /// entran cuando haya builds de esas plataformas).
    NotPe,
}

impl Architecture {
    pub fn name(&self) -> String {
        match self {
            Architecture::X64 => "x64".into(),
            Architecture::X86 => "x86 (32 bits)".into(),
            Architecture::Arm64 => "ARM64".into(),
            Architecture::Other(m) => format!("desconocida (0x{m:04x})"),
            Architecture::NotPe => tr!("no es una DLL de Windows"),
        }
    }
}

/// Lee el campo `Machine` del header COFF de un PE.
///
/// Función pura sobre los bytes del principio del archivo: es lo que la hace
/// verificable sin tener DLLs de cada arquitectura en el repositorio.
///
/// Estructura, que es toda la que se necesita: `MZ` en 0; el offset del header
/// PE en 0x3C (u32 LE); ahí `PE\0\0` y, justo después, el `Machine` (u16 LE).
pub fn pe_architecture(bytes: &[u8]) -> Architecture {
    if bytes.len() < 0x40 || &bytes[0..2] != b"MZ" {
        return Architecture::NotPe;
    }
    let off = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    // El offset lo escribe el archivo: un valor absurdo no puede indexar fuera.
    if off + 6 > bytes.len() || &bytes[off..off + 4] != b"PE\0\0" {
        return Architecture::NotPe;
    }
    match u16::from_le_bytes([bytes[off + 4], bytes[off + 5]]) {
        0x8664 => Architecture::X64,
        0x014c => Architecture::X86,
        0xAA64 => Architecture::Arm64,
        other_machine => Architecture::Other(other_machine),
    }
}

/// Lo mismo, leyendo sólo la cabeza del archivo (nunca el archivo entero: un
/// core son ~10 MB y esto corre en la pantalla de onboarding).
pub fn architecture_of(path: &Path) -> std::io::Result<Architecture> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut header = vec![0u8; 4096];
    let n = f.read(&mut header)?;
    header.truncate(n);
    Ok(pe_architecture(&header))
}

// ---------------------------------------------------------------------------
// Lo que el core dice de sí mismo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreInfo {
    /// `retro_api_version`. El runtime habla la 1.
    pub api: u32,
    pub name: String,
    pub version: String,
    /// Las extensiones que el core declara servir, en minúsculas y sin punto.
    pub extensions: Vec<String>,
}

/// Lee la línea `AYTHER_STATUS {...}` con `event=probe` de la salida del
/// runtime.
///
/// Pura, porque es la parte que se rompe: el runtime imprime otras cosas antes
/// (`[Config] Loaded from …`, `[CoreLoader] …`) y un parser que asuma «la
/// primera línea» funciona hasta el día que alguien agregue un log.
pub fn parse_probe(output: &str) -> Result<CoreInfo, String> {
    let line = output
        .lines()
        .filter_map(|l| l.trim().strip_prefix("AYTHER_STATUS "))
        .find(|j| j.contains("\"event\":\"probe\""))
        .ok_or_else(|| tr!("el runtime no informó el resultado del sondeo"))?;

    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("JSON inválido: {e}"))?;

    if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
        let reason = v
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("desconocido");
        return Err(reason.to_string());
    }
    Ok(CoreInfo {
        api: v.get("api").and_then(|a| a.as_u64()).unwrap_or(0) as u32,
        name: v
            .get("library_name")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        version: v
            .get("library_version")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        extensions: v
            .get("valid_extensions")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .split('|')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect(),
    })
}

impl CoreInfo {
    /// Las plataformas que este core sirve, deducidas de sus extensiones.
    ///
    /// El core no declara plataformas —libretro no tiene ese campo—, declara
    /// extensiones; y Play ya sabe traducir extensión a plataforma porque es
    /// como clasifica la biblioteca. Usar la MISMA tabla es lo que evita que el
    /// semáforo diga «compatible» para una ROM que el escáner clasificó de otra
    /// manera.
    pub fn platforms(&self) -> BTreeSet<String> {
        self.extensions
            .iter()
            .filter_map(|e| crate::library::platform_for_ext(e))
            .map(|p| p.to_string())
            .collect()
    }

    /// La usa el oráculo del sondeo, que es donde importa que las extensiones
    /// del core se traduzcan a NUESTROS nombres de consola: el `allow` no cubre `cargo test`: si el oráculo se va, el aviso vuelve.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn supports(&self, platform: &str) -> bool {
        self.platforms().iter().any(|p| p == platform)
    }
}

// ---------------------------------------------------------------------------
// Veredicto
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Sirve. Play puede lanzarlo.
    Valid(CoreInfo),
    /// El archivo no está donde dice la configuración.
    Missing(PathBuf),
    /// Está, pero es de otra arquitectura que la del runtime que lo va a cargar.
    ForeignArchitecture {
        core: Architecture,
        runtime: Architecture,
    },
    /// Carga, pero no exporta la interfaz libretro.
    NotACore(PathBuf),
    /// Es un core, pero de una API que este runtime no habla.
    ForeignApi { api: u32 },
    /// No se pudo ni sondear: falta el runtime, o murió sin decir nada.
    ProbeFailed(String),
}

impl Verdict {
    pub fn is_usable(&self) -> bool {
        matches!(self, Verdict::Valid(_))
    }

    /// Un renglón que dice qué pasó.
    pub fn title(&self) -> String {
        match self {
            Verdict::Valid(i) => tr!("Core válido: {} {}", i.name, i.version),
            Verdict::Missing(_) => tr!("No se encontró el core"),
            Verdict::ForeignArchitecture { .. } => tr!("El core es de otra arquitectura"),
            Verdict::NotACore(_) => tr!("Ese archivo no es un core"),
            Verdict::ForeignApi { .. } => {
                tr!("El core usa una versión de libretro que no soportamos")
            }
            Verdict::ProbeFailed(_) => tr!("No se pudo comprobar el core"),
        }
    }

    /// Y el renglón que dice **qué hacer**. La regla: si el jugador no puede
    /// actuar sobre el mensaje, el mensaje está mal escrito.
    pub fn remedy(&self) -> String {
        match self {
            Verdict::Valid(i) => {
                let p: Vec<String> = i.platforms().into_iter().collect();
                if p.is_empty() {
                    tr!(
                        "No declara extensiones conocidas; se aceptará igual, pero puede rechazar tus ROM."
                    )
                } else {
                    tr!("Sirve para: {}", p.join(", "))
                }
            }
            Verdict::Missing(p) => tr!(
                "Elegí el archivo del core en tu equipo. Buscábamos: {}",
                p.display()
            ),
            Verdict::ForeignArchitecture { core, runtime } => tr!(
                "Descargá la versión {} del core: la que elegiste es {}.",
                runtime.name(),
                core.name()
            ),
            Verdict::NotACore(_) => {
                tr!(
                    "Elegí un core de libretro (por ejemplo genesis_plus_gx_libretro), no otra DLL."
                )
            }
            Verdict::ForeignApi { api } => tr!(
                "Este core habla la API {} y Ayther habla la 1. Actualizá el core.",
                api
            ),
            Verdict::ProbeFailed(d) => tr!("Revisá la instalación de Ayther. Detalle: {}", d),
        }
    }
}

// ---------------------------------------------------------------------------
// La validación completa
// ---------------------------------------------------------------------------

/// La API de libretro que el runtime implementa. No es la ABI de AYTHER (1.10,
/// negociada aparte): es `retro_api_version`, que lleva años en 1.
pub const API_LIBRETRO: u32 = 1;

/// Valida `core` para el `runtime` dado, sin cargarlo en este proceso.
///
/// El orden importa: primero lo que se puede saber leyendo el archivo (barato y
/// específico), después el sondeo (un proceso). Al revés, un core x86 daría el
/// genérico «no carga» y perderíamos el único mensaje accionable que hay.
pub fn validate(runtime: &Path, core: &Path) -> Verdict {
    if !core.exists() {
        return Verdict::Missing(core.to_path_buf());
    }
    // Arquitectura: contra la del RUNTIME, que es el proceso que lo va a
    // cargar. Compararla contra la de Play sería casi siempre lo mismo y
    // silenciosamente falso el día que se distribuyan por separado.
    if let (Ok(a_core), Ok(a_rt)) = (architecture_of(core), architecture_of(runtime))
        && a_core != a_rt
        && a_core != Architecture::NotPe
        && a_rt != Architecture::NotPe
    {
        return Verdict::ForeignArchitecture {
            core: a_core,
            runtime: a_rt,
        };
    }

    let output = match std::process::Command::new(runtime)
        .arg("--probe-core")
        .arg(core)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return Verdict::ProbeFailed(tr!("no se pudo ejecutar {}: {}", runtime.display(), e));
        }
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    match parse_probe(&text) {
        Ok(info) if info.api != API_LIBRETRO => Verdict::ForeignApi { api: info.api },
        Ok(info) => Verdict::Valid(info),
        Err(reason) if reason == "no_es_libretro" => Verdict::NotACore(core.to_path_buf()),
        Err(reason) if reason == "no_carga" => {
            // Llegar acá con la arquitectura ya comprobada quiere decir que la
            // DLL está pero le falta una dependencia (el runtime de VC, por
            // ejemplo). Decirlo es la diferencia entre «no carga» y algo que se
            // puede buscar.
            Verdict::ProbeFailed(tr!(
                "el sistema no pudo cargar la DLL; puede faltarle una dependencia"
            ))
        }
        Err(reason) => Verdict::ProbeFailed(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arma un PE mínimo con la máquina pedida. Bytes de verdad, sin DLL.
    fn pe(machine: u16) -> Vec<u8> {
        let mut b = vec![0u8; 0x100];
        b[0] = b'M';
        b[1] = b'Z';
        let off: u32 = 0x80;
        b[0x3C..0x40].copy_from_slice(&off.to_le_bytes());
        let o = off as usize;
        b[o..o + 4].copy_from_slice(b"PE\0\0");
        b[o + 4..o + 6].copy_from_slice(&machine.to_le_bytes());
        b
    }

    #[test]
    fn the_architecture_comes_from_the_header_without_loading_anything() {
        assert_eq!(pe_architecture(&pe(0x8664)), Architecture::X64);
        assert_eq!(pe_architecture(&pe(0x014c)), Architecture::X86);
        assert_eq!(pe_architecture(&pe(0xAA64)), Architecture::Arm64);
        assert_eq!(pe_architecture(&pe(0x0200)), Architecture::Other(0x0200));
    }

    #[test]
    fn a_file_that_is_not_pe_does_not_pass_as_one() {
        assert_eq!(pe_architecture(b"no soy una dll"), Architecture::NotPe);
        assert_eq!(pe_architecture(&[]), Architecture::NotPe);
        // MZ pero sin header PE donde promete: un .exe de DOS, o basura.
        let mut b = pe(0x8664);
        b[0x80] = b'X';
        assert_eq!(pe_architecture(&b), Architecture::NotPe);
    }

    /// El offset del PE lo escribe el archivo: si apunta afuera, esto no puede
    /// romperse. Un panic acá es un cierre de Play provocado por un archivo
    /// que el jugador eligió.
    #[test]
    fn an_absurd_offset_does_not_panic() {
        let mut b = pe(0x8664);
        b[0x3C..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(pe_architecture(&b), Architecture::NotPe);
        let mut b2 = pe(0x8664);
        b2[0x3C..0x40].copy_from_slice(&0xFEu32.to_le_bytes()); // dentro, pero sin lugar
        assert_eq!(pe_architecture(&b2), Architecture::NotPe);
    }

    /// La salida real del runtime, con sus logs adelante.
    const EXIT_OK: &str = "[Config] Loaded from C:\\Users\\x\\config.toml\n\
        [CoreLoader] Loaded: cores\\genesis_plus_gx_libretro_ayther_x64.dll\n\
        AYTHER_STATUS {\"event\":\"probe\",\"ok\":true,\"api\":1,\
        \"library_name\":\"Genesis Plus GX\",\"library_version\":\"v1.7.4 752a6ff7\",\
        \"valid_extensions\":\"m3u|mdx|md|smd|gen|bin|cue|iso|chd|bms|sms|gg|sg|68k|sgd\"}\n";

    #[test]
    fn the_probe_is_read_even_with_logs_before_it() {
        let i = parse_probe(EXIT_OK).expect("debería parsear");
        assert_eq!(i.api, 1);
        assert_eq!(i.name, "Genesis Plus GX");
        assert!(i.extensions.contains(&"md".to_string()));
    }

    #[test]
    fn platforms_come_from_the_same_table_as_the_library() {
        let i = parse_probe(EXIT_OK).unwrap();
        let p = i.platforms();
        // Genesis Plus GX sirve Mega Drive y Master System / Game Gear.
        assert!(i.supports("Mega Drive"), "plataformas detectadas: {p:?}");
        assert!(i.supports("Master System"));
        // Y no las que no: el semáforo de #589 tiene que poder decir que no.
        assert!(!i.supports("SNES"));
        assert!(!i.supports("N64"));
    }

    #[test]
    fn a_failed_probe_returns_its_reason_and_not_an_empty_core() {
        let s = "AYTHER_STATUS {\"event\":\"probe\",\"ok\":false,\"reason\":\"no_es_libretro\"}\n";
        assert_eq!(parse_probe(s), Err("no_es_libretro".into()));
        // Sin línea de sondeo: el runtime murió antes de decir nada. No puede
        // leerse como «core válido con nombre vacío».
        assert!(parse_probe("[CoreLoader] boom\n").is_err());
        // Y un evento que no es el sondeo tampoco cuenta.
        assert!(parse_probe("AYTHER_STATUS {\"event\":\"ready\"}\n").is_err());
    }

    #[test]
    fn every_bad_verdict_says_what_to_do() {
        let rejected_verdicts = [
            Verdict::Missing(PathBuf::from("c:\\cores\\x.dll")),
            Verdict::ForeignArchitecture {
                core: Architecture::X86,
                runtime: Architecture::X64,
            },
            Verdict::NotACore(PathBuf::from("SDL3.dll")),
            Verdict::ForeignApi { api: 2 },
            Verdict::ProbeFailed("boom".into()),
        ];
        for v in rejected_verdicts {
            assert!(!v.is_usable());
            assert!(!v.title().is_empty(), "{v:?} sin título");
            let r = v.remedy();
            assert!(r.len() > 20, "el remedio de {v:?} no dice qué hacer: «{r}»");
        }
    }

    // -----------------------------------------------------------------------
    // Contra los binarios de verdad, cuando esta máquina los tiene.
    //
    // Los de arriba prueban las decisiones; éste prueba que el runtime y Play
    // sigan hablando el mismo protocolo. Es el que se rompe cuando alguien
    // cambia un código de salida del lado C++ y nadie lo nota del lado Rust.
    //
    // El runtime se construye y los cores se bajan aparte (`core.lock`), así
    // que la prueba requiere preparación explícita y se ejecuta con --ignored; un
    // resultado ignorado no constituye evidencia de integración.
    // -----------------------------------------------------------------------
    #[test]
    #[ignore = "integration: requires AYTHER_RUNTIME_EXE and AYTHER_TEST_CORE; release runs --ignored"]
    fn the_probe_protocol_works_against_the_real_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let rt = PathBuf::from(
            std::env::var_os("AYTHER_RUNTIME_EXE").expect("AYTHER_RUNTIME_EXE is required"),
        );
        let core = PathBuf::from(
            std::env::var_os("AYTHER_TEST_CORE").expect("AYTHER_TEST_CORE is required"),
        );
        assert!(rt.is_file(), "missing Runtime: {}", rt.display());
        assert!(core.is_file(), "missing core: {}", core.display());
        match validate(&rt, &core) {
            Verdict::Valid(i) => {
                assert_eq!(i.api, API_LIBRETRO);
                assert!(!i.name.is_empty(), "el core no se nombró");
                assert!(
                    !i.extensions.is_empty(),
                    "the fixture must advertise extensions"
                );
            }
            other => panic!("el core del fork no validó: {other:?} — {}", other.remedy()),
        }

        // Y uno que no existe, que NO puede confundirse con el anterior.
        assert!(matches!(
            validate(&rt, &temp.path().join("no-existe.dll")),
            Verdict::Missing(_)
        ));
    }

    /// El mensaje de arquitectura nombra las DOS: sin la del runtime, «el core
    /// es x86» no le dice al jugador cuál bajar.
    #[test]
    fn the_architecture_error_names_the_one_to_download() {
        let v = Verdict::ForeignArchitecture {
            core: Architecture::X86,
            runtime: Architecture::X64,
        };
        let r = v.remedy();
        assert!(r.contains("x64"), "{r}");
        assert!(r.contains("x86"), "{r}");
    }
}
