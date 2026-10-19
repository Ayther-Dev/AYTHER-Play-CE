//! config.rs — persistent launcher settings.
//!
//! Stored at:
//!   Windows  %APPDATA%\Ayther\play_config.toml
//!   Linux    ~/.config/Ayther/play_config.toml
//!   macOS    ~/Library/Application Support/Ayther/play_config.toml
//!
//! Falls back to sensible defaults relative to the launcher executable when
//! no config file exists (e.g. first run from the build directory during dev).

use std::path::{Path, PathBuf};

fn default_language() -> String {
    "auto".into()
}
use serde::{Deserialize, Serialize};

fn default_log() -> String {
    "info".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Directory to scan for ROMs.
    ///
    /// #588 — quedó como la carpeta PRINCIPAL (la primera de `rom_dirs`). Sigue
    /// existiendo porque es lo que leen las partes que todavía piensan en una
    /// sola carpeta, y porque una config vieja no tiene `rom_dirs`.
    pub rom_dir: PathBuf,

    /// #588 — todas las carpetas de ROM. Una biblioteca real está repartida:
    /// una carpeta por consola, o un disco externo aparte. Con una sola
    /// carpeta, la mitad de los juegos no aparece y no hay manera de decir
    /// dónde están los otros.
    #[serde(default)]
    pub rom_dirs: Vec<PathBuf>,

    /// #588 — el wizard de primer arranque ya se completó.
    #[serde(default)]
    #[serde(alias = "onboarding_completo")]
    pub onboarding_complete: bool,

    /// #599 — el perfil de SALIDA global (CRT · LCD · píxel exacto…).
    ///
    /// Vacío = no elegido, y ahí manda lo que el pack recomienda. Guardar el
    /// resuelto convertiría la recomendación del pack en la elección del
    /// usuario a la primera partida, y el pack siguiente ya no podría
    /// recomendar nada — es la misma razón por la que `LaunchConfig.output`
    /// también se guarda vacío.
    #[serde(default)]
    pub output_global: String,

    /// #592 — la biblioteca se vio por última vez como LISTA (y no como
    /// grilla, que es el default del diseño).
    ///
    /// Se recuerda porque es una preferencia de lectura, no un modo temporal:
    /// quien prefiere la lista la prefiere siempre, y volver a la grilla en cada
    /// arranque se lee como que Play no lo escuchó.
    #[serde(default)]
    #[serde(alias = "vista_lista")]
    pub list_view: bool,

    /// #588/#590 — versión de los términos BYOR/BYOC que el jugador aceptó. Es
    /// una versión y no un `bool` para que unos términos nuevos se vuelvan a
    /// mostrar.
    #[serde(default)]
    #[serde(alias = "terminos_aceptados")]
    pub terms_accepted: Option<String>,

    /// #590 — cuándo los aceptó, en UTC (`2026-08-26T22:14:03Z`).
    ///
    /// Va al lado de la versión y no en su lugar porque las dos responden
    /// preguntas distintas —«¿aceptó los términos que rigen hoy?» y «¿cuándo?»—
    /// y ninguna sirve sola: una fecha sin versión no dice qué se aceptó, y una
    /// versión sin fecha no es un registro. Por eso también se escriben juntas
    /// y `terminos_vigentes()` las mira juntas.
    #[serde(default)]
    #[serde(alias = "terminos_fecha")]
    pub terms_date: Option<String>,

    /// Path to the `ayther_runtime` executable.
    pub runtime_path: PathBuf,

    /// Path to the default Libretro core DLL/SO (BYOC).
    pub default_core: PathBuf,

    /// #589 — el core de cada plataforma, por nombre de plataforma tal como lo
    /// escribe la biblioteca («Mega Drive», «SNES»…).
    ///
    /// Play es BYOC y un jugador con ROM de varias consolas necesita un core
    /// por consola; con un solo `default_core`, la mitad de su biblioteca no
    /// arranca y el error aparece recién al lanzar. Lo que no está acá cae a
    /// `default_core`, así que una instalación de una sola plataforma no ve
    /// esta sección nunca.
    #[serde(default)]
    pub cores: std::collections::BTreeMap<String, PathBuf>,

    /// Path to the JSON library cache file.
    pub lib_path: PathBuf,
    /// SQLite catalog; legacy JSON remains available for migration/recovery.
    #[serde(default)]
    pub catalog_path: Option<PathBuf>,

    /// Dónde viven los packs instalados. En CE llegan desde un archivo del
    /// disco —la pantalla de Ajustes o `--install`—: no hay descarga que los
    /// deje acá.
    pub packs_install_dir: PathBuf,

    /// #678 — la CARPETA DE PACKS: lo que aparezca acá está disponible sin
    /// instalar nada, igual que una ROM en la carpeta de ROMs. Es el otro
    /// camino: **instalar** es para el pack que se quiere tener puesto; la
    /// carpeta es para la colección que se quiere tener a mano.
    ///
    /// Es una lista por lo mismo que `rom_dirs` —una colección real vive
    /// repartida— aunque la pantalla de Ajustes designe UNA: las demás se
    /// agregan a mano al TOML y el escaneo las lee todas, en orden.
    #[serde(default)]
    pub pack_dirs: Vec<PathBuf>,
    /// Explicit public registries for individual pack paths. Passed to Runtime;
    /// Play CE neither discovers private keys nor authenticates signers itself.
    #[serde(default)]
    pub pack_trust_registries: std::collections::BTreeMap<PathBuf, PathBuf>,
    /// Los controles del JUEGO: qué tecla y qué botón hace cada acción del
    /// RetroPad en la partida (Ajustes › Controles del juego). Sólo lo
    /// distinto de fábrica, por clave del contrato (`controls.rs`); vacío =
    /// nunca se tocó, y la flag `--input-map` ni viaja.
    #[serde(default)]
    #[serde(alias = "controles_teclas")]
    pub control_keys: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    #[serde(alias = "controles_botones")]
    pub control_buttons: std::collections::BTreeMap<String, String>,

    /// Cuánto dice el log: `error` · `aviso` · `info` · `detalle` (#621).
    /// `AYTHER_PLAY_LOG` lo pisa sin tocar el archivo.
    ///
    /// Con `default`, como todo campo que se agrega después: sin eso, el
    /// `play_config.toml` de quien ya venía usando Play deja de parsear entero
    /// y el jugador arranca con rutas por defecto, sin biblioteca y sin core —
    /// su archivo intacto y su Play vacío.
    #[serde(default = "default_log")]
    pub log: String,

    /// Directory for local savestates used by SRAM cloud sync.
    pub saves_dir: PathBuf,
    /// #313: dónde quedan las opiniones del jugador y su decisión sobre la
    /// pregunta. En el árbol de DATOS y no en el de config: son cosas que el
    /// jugador escribió, no ajustes.
    pub feedback_dir: PathBuf,

    /// #585 — idioma de la interfaz: `"es"`, `"en"` o `"auto"` (el del
    /// sistema). El español es el idioma base (los msgid están en español),
    /// así que `auto` en una máquina sin locale conocido cae ahí.
    #[serde(default = "default_language")]
    pub language: String,

    /// Optional path to a No-Intro XML DAT file for canonical name lookup.
    /// If absent, filenames are used as display names.
    pub dat_path: Option<PathBuf>,

    /// Directory containing cover art images.
    /// Naming conventions checked in order:
    ///   {covers_dir}/{CRC32_UPPER}.png  (most reliable)
    ///   {covers_dir}/{CRC32_UPPER}.jpg
    ///   {covers_dir}/{slug_name}.png    (slug = lowercase, spaces→underscores)
    pub covers_dir: PathBuf,
}

impl Config {
    pub fn catalog_path(&self) -> PathBuf {
        self.catalog_path.clone().unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Ayther")
                .join("catalog.sqlite3")
        })
    }
    /// Load from the platform config directory, or build sensible defaults.
    pub fn load() -> Self {
        let config_path = Self::config_file();
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(text) => match toml::from_str::<Config>(&text) {
                    Ok(cfg) => return cfg,
                    // #588 — esto se comía en silencio, y el silencio es el
                    // problema: una coma de más en el TOML dejaba a Play
                    // corriendo con rutas por defecto, sin biblioteca y sin
                    // core, exactamente igual que un primer arranque. El
                    // jugador veía «no hay ROMs» y su archivo intacto.
                    Err(e) => crate::log_warning!(
                        "config",
                        "{} no se pudo leer ({e}); se usan valores por defecto",
                        config_path.display()
                    ),
                },
                Err(e) => {
                    crate::log_warning!("config", "no se pudo abrir {}: {e}", config_path.display())
                }
            }
        }
        // First run / missing config → defaults.
        Self::defaults()
    }

    /// Dónde está el runtime, de verdad.
    ///
    /// `runtime_path` es la respuesta corta y sirve en una instalación: Play y
    /// el Runtime viven en la misma carpeta. **En el árbol de desarrollo no**:
    /// Play lo compila Cargo en `target/debug/` y el Runtime lo compila CMake en
    /// `build/bin/`, así que el default apuntaba a un archivo que no existe y el
    /// paso del core del wizard fallaba con «no se pudo ejecutar
    /// …/target/debug/ayther_runtime.exe (os error 2)». El sondeo del core corre
    /// FUERA de proceso —es lo que impide que una DLL ajena se lleve puesto a
    /// Play (#589)—, así que sin runtime no hay veredicto posible.
    ///
    /// El orden es el de la autoridad: `AYTHER_RUNTIME`, que es cómo se apunta
    /// a un build sin tocar la configuración; después el Runtime que viaja con
    /// Play en su carpeta; después lo configurado, si existe; y recién ahí el
    /// árbol de desarrollo. El adyacente le gana a lo configurado porque es el
    /// que se distribuyó y validó con ESTE Play: una ruta guardada por una
    /// versión anterior seguía existiendo y lanzaba un Runtime viejo (beta.8,
    /// sin las correcciones de la spec 002) aunque la carpeta trajera beta.11.
    /// Si no aparece ninguno se devuelve el configurado igual, para que el
    /// error NOMBRE lo que se buscaba en vez de un camino inventado.
    pub fn runtime(&self) -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let env = std::env::var_os("AYTHER_RUNTIME").map(PathBuf::from);
        Self::runtime_at(&self.runtime_path, &exe_dir, env.as_deref())
    }

    pub fn pack_trust_registry(&self, pack: Option<&Path>) -> Option<&Path> {
        self.pack_trust_registries.get(pack?).map(PathBuf::as_path)
    }

    /// El nombre del ejecutable del runtime en esta plataforma.
    pub fn runtime_executable_name() -> &'static str {
        if cfg!(windows) {
            "ayther_runtime.exe"
        } else {
            "ayther_runtime"
        }
    }

    /// La cascada, sin tocar el proceso. Pura para poder probarla: la de arriba
    /// depende de dónde está el `.exe` que corre, que en un test es el binario
    /// del test.
    pub fn runtime_at(configured: &Path, exe_dir: &Path, env: Option<&Path>) -> PathBuf {
        if let Some(e) = env
            && e.is_file()
        {
            return e.to_path_buf();
        }
        let name = Self::runtime_executable_name();
        let adjacent_runtime = exe_dir.join(name);
        if adjacent_runtime.is_file() {
            return adjacent_runtime;
        }
        if configured.is_file() {
            return configured.to_path_buf();
        }
        // El árbol de desarrollo: `target/debug/` está a dos o tres niveles de
        // la raíz del repositorio, y el runtime lo deja CMake en `build/bin/`.
        for ancestor in exe_dir.ancestors().take(4) {
            let c = ancestor.join("build").join("bin").join(name);
            if c.is_file() {
                return c;
            }
        }
        configured.to_path_buf()
    }

    /// Dónde se dejan los diagnósticos: pánicos de Play y lo que el modo
    /// seguro (#601) necesita mirar después de un cierre feo.
    ///
    /// Junto a las sesiones y no en una carpeta aparte del sistema: quien pide
    /// ayuda tiene que poder mandar UN directorio.
    pub fn diagnostics_dir(&self) -> PathBuf {
        // `sessions_dir()` YA es la raíz del árbol de datos —el nombre engaña,
        // las sesiones cuelgan de ahí— así que esto es hermano de `sessions/`
        // y no su tío.
        self.sessions_dir().join("diagnostics")
    }

    /// #596: la raíz del árbol de DATOS, donde viven las sesiones.
    ///
    /// Se deriva de `saves_dir` en vez de volver a preguntarle a `dirs` porque
    /// el jugador puede haber movido su carpeta de datos: preguntando de nuevo,
    /// las sesiones quedarían en la de fábrica y las partidas en la suya.
    pub fn sessions_dir(&self) -> PathBuf {
        self.saves_dir
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.saves_dir.clone())
    }

    /// Persist the current config to disk (best-effort — errors are logged only).
    pub fn save(&self) {
        self.save_to(&Self::config_file());
    }

    /// Lo mismo, a una ruta dada. Existe para que se pueda PROBAR que el
    /// archivo se escribe y se vuelve a leer igual: el escritor de verdad
    /// apunta a la configuración del jugador, y un test que la pisara sería
    /// peor que no tener test.
    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(path, text) {
                    eprintln!("[config] save failed: {e}");
                }
            }
            Err(e) => eprintln!("[config] serialize failed: {e}"),
        }
    }

    // ---- Paths ---------------------------------------------------------------

    pub fn config_file() -> PathBuf {
        Self::ayther_config_dir().join("play_config.toml")
    }

    /// #311: la carpeta de configuracion, para lo que se guarda por juego.
    pub fn config_dir() -> PathBuf {
        Self::ayther_config_dir()
    }

    fn ayther_config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Ayther")
    }

    pub(crate) fn defaults() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Ayther");

        Config {
            language: default_language(),
            rom_dir: exe_dir.join("roms"),
            runtime_path: exe_dir.join(if cfg!(windows) {
                "ayther_runtime.exe"
            } else {
                "ayther_runtime"
            }),
            default_core: exe_dir.join("cores").join(if cfg!(windows) {
                "genesis_plus_gx_libretro.dll"
            } else if cfg!(target_os = "macos") {
                "genesis_plus_gx_libretro.dylib"
            } else {
                "genesis_plus_gx_libretro.so"
            }),
            rom_dirs: Vec::new(),
            onboarding_complete: false,
            list_view: false,             // #592: la grilla es el default del diseño
            output_global: String::new(), // #599: manda el pack
            terms_accepted: None,
            terms_date: None,
            cores: Default::default(),
            lib_path: data_dir.join("library.json"),
            catalog_path: None,
            packs_install_dir: data_dir.join("packs"),
            pack_dirs: Vec::new(),
            pack_trust_registries: Default::default(),
            control_keys: Default::default(),
            control_buttons: Default::default(),
            log: "info".into(),
            // #623 — apagada. Es el default correcto y el único que se puede
            // defender: nadie queda adentro por no haber leído.
            saves_dir: data_dir.join("saves"),
            feedback_dir: data_dir.join("feedback"),
            dat_path: None,
            covers_dir: data_dir.join("covers"),
        }
    }
}

/// Las carpetas de ROM a escanear (#588).
///
/// `rom_dirs` si el wizard la escribió; si no, la `rom_dir` de siempre. Una
/// config escrita antes de #588 no tiene `rom_dirs` y tiene que seguir
/// funcionando sin que nadie la toque.
pub fn rom_folders(cfg: &Config) -> Vec<PathBuf> {
    if cfg.rom_dirs.is_empty() {
        vec![cfg.rom_dir.clone()]
    } else {
        cfg.rom_dirs.clone()
    }
}

/// #592/#642: el core con el que Play correría esta plataforma, si hay alguno
/// configurado.
///
/// Devuelve la RUTA y no un `bool` porque «¿se puede jugar esto?» no se contesta
/// con el archivo. Hasta #642 sí: esto era `hay_core_para`, comprobaba que el
/// archivo existiera y de ahí salía el «Compatible» de la ficha — así que un
/// core de 32 bits, uno corrupto o uno de otra consola daban compatible y el
/// juego no arrancaba. Lo que se resuelve acá es de QUÉ core estamos hablando;
/// si sirve, lo dice el sondeo (`cores::eje`).
///
/// NO se puede responder con `resolve_core`: ésa siempre devuelve algo —cae al
/// `default_core`— porque su trabajo es lanzar, no diagnosticar. La tarjeta
/// necesita lo contrario: saber cuándo NO va a poder, para decirlo ANTES de que
/// el jugador apriete A y se encuentre con un error.
///
/// **`cores` MANDA cuando existe, y el `default_core` no lo salva.** El wizard
/// no anota el core que el jugador eligió bajo una plataforma: lo SONDEA y le
/// anota una entrada por cada plataforma que ese core sirve de verdad
/// (`onboarding.rs`, #589). Así que un mapa no vacío es la respuesta completa a
/// esta pregunta, y caer al `default_core` cuando la plataforma no está sería
/// tirar esa información: quien tiene sólo un core de Mega Drive vería sus ROMs
/// de SNES como jugables y se enteraría al apretar A. Es exactamente el caso
/// que el estado «Falta el core» existe para mostrar.
///
/// Con el mapa VACÍO no hay nada mejor que el default: es una configuración
/// anterior a #589, o escrita a mano. Ahí decir «falta el core» de todo sería
/// peor que no decir nada.
///
/// Que el archivo EXISTA no se comprueba acá: es la primera pregunta de
/// `cores::eje`, donde tiene un estado propio («Falta el core») con su remedio.
pub fn core_for(cfg: &Config, platform: &str) -> Option<PathBuf> {
    if !cfg.cores.is_empty() {
        return cfg.cores.get(platform).cloned();
    }
    Some(cfg.default_core.clone())
}

/// El core con el que se lanza esta ROM: el de su plataforma si el jugador
/// configuró uno, si no el core por defecto (#589).
///
/// La plataforma se deduce de la extensión con la MISMA tabla que usa el
/// escáner de la biblioteca. Si se dedujera acá aparte, un juego podría
/// mostrarse como «Mega Drive» en la grilla y lanzarse con el core de SNES.
///
/// Sin llamador en la UI —la que decide qué mostrar es `cores::eje`— y viva por
/// su oráculo, que fija la precedencia por extensión: el `allow` no cubre
/// `cargo test`: si el oráculo se va, el aviso vuelve.
#[cfg_attr(not(test), allow(dead_code))]
pub fn resolve_core(cfg: &Config, game_path: &Path) -> PathBuf {
    let platform = game_path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(crate::library::platform_for_ext);
    if let Some(p) = platform
        && let Some(core) = cfg.cores.get(p)
    {
        return core.clone();
    }
    cfg.default_core.clone()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn public_registry_is_saved_and_only_used_for_its_selected_pack() {
        let mut config = Config::defaults();
        let pack = PathBuf::from("packs/Golden Axe.ay");
        let registry = PathBuf::from("certificates/golden-axe.toml");
        config
            .pack_trust_registries
            .insert(pack.clone(), registry.clone());
        let restored: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(
            restored.pack_trust_registry(Some(&pack)),
            Some(registry.as_path())
        );
        assert_eq!(
            restored.pack_trust_registry(Some(Path::new("packs/other.ay"))),
            None
        );
        assert_eq!(restored.pack_trust_registry(None), None);
    }

    /// El runtime también se encuentra en el ÁRBOL DE DESARROLLO.
    ///
    /// El defecto: `runtime_path` cae por defecto al lado del `.exe` de Play, y
    /// eso es cierto en una instalación pero no acá — Cargo deja Play en
    /// `target/debug/` y CMake deja el Runtime en `build/bin/`. El paso del core
    /// del wizard fallaba con «no se pudo ejecutar …/target/debug/
    /// ayther_runtime.exe (os error 2)» y marcaba INCOMPATIBLE un core que
    /// nunca se llegó a abrir.
    #[test]
    fn the_runtime_shows_up_in_the_dev_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let exe_dir = root.join("target").join("debug");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let real = root
            .join("build")
            .join("bin")
            .join(Config::runtime_executable_name());
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, b"no importa").unwrap();

        // Lo configurado no existe: es el default de una instalación.
        let configured = exe_dir.join(Config::runtime_executable_name());
        assert_eq!(Config::runtime_at(&configured, &exe_dir, None), real);
    }

    /// Y la cascada respeta quién manda: la variable de entorno, después lo
    /// configurado, y recién ahí el árbol. Al revés, un build viejo en
    /// `build/bin/` le ganaría al runtime que el jugador eligió.
    #[test]
    fn the_variable_rules_then_the_configured_value() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let exe_dir = root.join("target").join("debug");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let development_runtime = root
            .join("build")
            .join("bin")
            .join(Config::runtime_executable_name());
        std::fs::create_dir_all(development_runtime.parent().unwrap()).unwrap();
        std::fs::write(&development_runtime, b"runtime").unwrap();

        let selected = root.join("el-que-eligio.exe");
        std::fs::write(&selected, b"elegido").unwrap();
        assert_eq!(Config::runtime_at(&selected, &exe_dir, None), selected);

        let from_env = root.join("el-del-entorno.exe");
        std::fs::write(&from_env, b"entorno").unwrap();
        assert_eq!(
            Config::runtime_at(&selected, &exe_dir, Some(&from_env)),
            from_env,
            "la variable de entorno apunta a un build sin tocar la configuración"
        );
        let missing = root.join("no-esta.exe");
        assert_eq!(
            Config::runtime_at(&missing, &exe_dir, Some(&from_env)),
            from_env,
            "la variable de entorno tiene que ganarle al árbol"
        );
    }

    /// El Runtime que viaja con Play le gana a una ruta guardada que todavía
    /// existe: la dejó una versión anterior y lanzaría un Runtime viejo.
    #[test]
    fn the_adjacent_runtime_wins_over_a_stale_configured_one() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let exe_dir = root.join("play");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let adjacent = exe_dir.join(Config::runtime_executable_name());
        std::fs::write(&adjacent, b"beta.11").unwrap();
        let stale = root
            .join("runtime-viejo")
            .join(Config::runtime_executable_name());
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, b"beta.8").unwrap();

        assert_eq!(Config::runtime_at(&stale, &exe_dir, None), adjacent);
        let from_env = root.join("el-del-entorno.exe");
        std::fs::write(&from_env, b"entorno").unwrap();
        assert_eq!(
            Config::runtime_at(&stale, &exe_dir, Some(&from_env)),
            from_env,
            "la variable de entorno sigue mandando sobre el adyacente"
        );
    }

    /// Sin ningún runtime, devuelve el CONFIGURADO. El sondeo va a fallar igual
    /// —no hay con qué sondear— pero el mensaje nombra lo que se buscaba en vez
    /// de una ruta inventada, que es lo único accionable que queda.
    #[test]
    fn without_a_runtime_the_error_names_what_was_looked_for() {
        let tmp = tempfile::tempdir().unwrap();
        let exe_dir = tmp.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let configured = exe_dir.join(Config::runtime_executable_name());
        assert_eq!(Config::runtime_at(&configured, &exe_dir, None), configured);
    }

    /// Un `play_config.toml` escrito antes de #588/#589 no tiene `rom_dirs`,
    /// `cores`, `onboarding_completo` ni `terminos_aceptados`. Tiene que seguir
    /// cargando: si `Config::load()` falla al deserializar, cae a
    /// `defaults()` **en silencio** y el jugador pierde su configuración
    /// entera sin un mensaje.
    #[test]
    fn an_old_config_still_loads() {
        let previous = r#"
rom_dir = "D:/ROMs"
runtime_path = "ayther_runtime.exe"
default_core = "cores/gpgx.dll"
lib_path = "library.json"
packs_install_dir = "packs"
log = "info"
saves_dir = "saves"
feedback_dir = "feedback"
covers_dir = "covers"
"#;
        let cfg: Config = toml::from_str(previous).expect("una config vieja dejó de cargar");
        assert_eq!(cfg.rom_dir, PathBuf::from("D:/ROMs"));
        assert!(cfg.rom_dirs.is_empty());
        assert!(!cfg.onboarding_complete);
        assert_eq!(
            cfg.language, "auto",
            "el idioma sin escribir tiene que ser `auto`"
        );
        // Y la carpeta a escanear sigue siendo la que decía.
        assert_eq!(rom_folders(&cfg), vec![PathBuf::from("D:/ROMs")]);
    }

    /// #592/#642 — de QUÉ core estamos hablando para esta consola.
    ///
    /// Que el archivo exista y que el core sirva se contestan en `cores::eje`,
    /// con el sondeo. Lo que se fija acá es la precedencia, que es de esta
    /// función: el mapa manda y el default no lo desmiente.
    #[test]
    fn the_core_map_rules_over_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("gpgx.dll");

        let mut c = Config::defaults();
        c.default_core = real.clone();

        // Sin mapa de cores no hay mejor información que el default.
        assert!(c.cores.is_empty());
        assert_eq!(
            core_for(&c, "SNES"),
            Some(real.clone()),
            "config anterior a #589: manda el default"
        );

        // Con el mapa, el de la plataforma.
        let md = dir.path().join("md.dll");
        c.cores.insert("Mega Drive".into(), md.clone());
        assert_eq!(core_for(&c, "Mega Drive"), Some(md));

        // Y EL CASO QUE IMPORTA: con el mapa lleno, una plataforma que no está
        // NO se salva por el default. El wizard anota una entrada por cada
        // plataforma que el core sirve de verdad (#589), así que «no está en el
        // mapa» significa «este core no la corre». Caer al default acá le
        // mostraría al jugador sus ROMs de SNES como jugables teniendo sólo un
        // core de Mega Drive, y el error saldría al apretar A.
        assert_eq!(
            core_for(&c, "SNES"),
            None,
            "el mapa de cores manda: el default no puede desmentirlo"
        );
    }

    /// #592 — la vista elegida sobrevive al reinicio, que es todo el punto de
    /// guardarla: volver a la grilla en cada arranque se lee como que Play no
    /// escuchó.
    #[test]
    fn the_library_view_is_remembered() {
        let mut c = Config::defaults();
        assert!(!c.list_view, "el default del diseño es la GRILLA");
        c.list_view = true;

        let loaded_config: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert!(
            loaded_config.list_view,
            "la lista tiene que sobrevivir al reinicio"
        );
    }

    /// Lo que el wizard escribe se lee de vuelta igual. Un round-trip roto se
    /// ve como «la configuración no se guardó», que es lo que pasaba cuando
    /// nadie llamaba a `save()`.
    #[test]
    fn what_is_saved_is_what_is_read() {
        let mut c = Config::defaults();
        c.rom_dirs = vec![PathBuf::from("D:/ROMs"), PathBuf::from("E:/Mas")];
        c.cores
            .insert("Mega Drive".into(), PathBuf::from("C:/c/gpgx.dll"));
        c.onboarding_complete = true;
        c.terms_accepted = Some("1.0".into());
        c.terms_date = Some("2026-08-26T22:14:03Z".into());
        c.language = "en".into();

        let text = toml::to_string_pretty(&c).unwrap();
        let loaded_config: Config = toml::from_str(&text).unwrap();
        assert_eq!(loaded_config.rom_dirs, c.rom_dirs);
        assert_eq!(loaded_config.cores, c.cores);
        assert!(loaded_config.onboarding_complete);
        assert_eq!(loaded_config.terms_accepted.as_deref(), Some("1.0"));
        assert_eq!(
            loaded_config.terms_date.as_deref(),
            Some("2026-08-26T22:14:03Z")
        );
        assert_eq!(loaded_config.language, "en");
    }

    /// #590 — el registro de aceptación tiene que sobrevivir a reinstalar
    /// Play, y lo único que lo garantiza es DÓNDE vive: junto al ejecutable, un
    /// instalador que borra su carpeta se lo lleva puesto y al jugador se le
    /// vuelven a pedir los términos que ya aceptó. Va en la carpeta de
    /// configuración del usuario (`%APPDATA%\Ayther` en Windows), que ningún
    /// desinstalador toca.
    #[test]
    fn the_config_does_not_live_next_to_the_executable() {
        let cfg = Config::config_file();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .expect("sin ruta del ejecutable no se puede afirmar nada");
        assert!(
            !cfg.starts_with(&exe_dir),
            "play_config.toml vive junto al ejecutable ({}): reinstalar Play borraría \
             la aceptación de los términos",
            cfg.display()
        );
        assert_eq!(cfg.file_name().unwrap(), "play_config.toml");
    }

    /// El core por plataforma gana sobre el de por defecto, y una plataforma
    /// sin core configurado cae al de siempre (#589).
    #[test]
    fn the_core_is_resolved_per_platform() {
        let mut c = Config::defaults();
        c.default_core = PathBuf::from("por-defecto.dll");
        c.cores.insert("SNES".into(), PathBuf::from("snes9x.dll"));
        assert_eq!(
            resolve_core(&c, Path::new("j/Sonic.md")),
            PathBuf::from("por-defecto.dll")
        );
        assert_eq!(
            resolve_core(&c, Path::new("j/Mario.sfc")),
            PathBuf::from("snes9x.dll")
        );
        // Una extensión que no clasifica tampoco puede quedarse sin core.
        assert_eq!(
            resolve_core(&c, Path::new("j/raro.xyz")),
            PathBuf::from("por-defecto.dll")
        );
    }
}
