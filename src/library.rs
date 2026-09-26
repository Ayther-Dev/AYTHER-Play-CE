//! library.rs — ROM scanner + JSON-backed library cache.
//!
//! # Scan pipeline (M5b)
//!
//!   walkdir (single-threaded) → filter by ROM extension
//!   rayon par_iter            → CRC32 hash + pack discovery + cover path
//!   main thread               → DAT name lookup + pack manifest read
//!   main thread               → cover image loading (slint::Image is !Send)
//!
//! # Persistence
//!
//! JSON flat-file keyed on file path.  Idempotent upsert preserves any extra
//! metadata (No-Intro names, play-time) across re-scans.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Señal de cancelación, compartida entre quien pide parar (la UI) y quien
/// obedece (el escaneo). Un `bool` atómico y no un canal porque el escaneo no
/// necesita RECIBIR nada: sólo preguntar «¿sigo?» entre archivo y archivo.
pub type CancelToken = Arc<AtomicBool>;

/// Cuánto tamaño se lee por vuelta al hashear. 1 MiB es el punto donde el
/// syscall deja de dominar y la memoria todavía no importa: lo que se retiró
/// fue el archivo ENTERO en RAM, no la cantidad de vueltas.
const CHUNK: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Game model
// ---------------------------------------------------------------------------

/// El grado de compatibilidad del pack de esta ROM (#595).
///
/// **Espeja `ayther_core::pack_validate::CompatGrade`**, y eso es el punto de
/// #595: la matriz de cuatro grados tiene UNA implementación (#530), y Play
/// contesta lo mismo que el SDK y que el Hub porque los tres le preguntan al
/// mismo validador. Antes esto era un vocabulario propio —Ok/Corrupt/Mismatch—
/// derivado de si el manifest se podía leer, así que Play podía decir «Ok» de un
/// pack que `ay_pack validate` había rechazado.
///
/// `SinPack` no es un grado: es no tener nada que graduar, y se juega el
/// original. Va en el 0 para que el `int` que cruza a Slint siga ordenado de
/// «nada» a «peor».
///
/// Los valores se mantienen en sincronía con el `pack-status` de
/// `appwindow.slint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum PackStatus {
    /// El default es «sin pack» y no «exacto»: lo que falta no se da por bueno.
    #[default]
    #[serde(alias = "SinPack")]
    NoPack = 0,
    #[serde(alias = "Exacto")]
    Exact = 1,
    #[serde(alias = "Advertencias")]
    Warnings = 2,
    #[serde(alias = "NoVerificado")]
    Unverified = 3,
    Incompatible = 4,
}

impl PackStatus {
    pub fn as_int(self) -> i32 {
        self as i32
    }

    /// El grado del core, traducido. Es la ÚNICA conversión: si mañana el core
    /// agrega un grado, esto no compila y alguien tiene que decidir qué hacer —
    /// que es exactamente lo que un `_ => Default` se comería.
    pub fn from_grade(g: ayther_core::pack_validate::CompatGrade) -> Self {
        use ayther_core::pack_validate::CompatGrade as G;
        match g {
            G::Exact => PackStatus::Exact,
            G::Warnings => PackStatus::Warnings,
            G::Experimental => PackStatus::Unverified,
            G::Incompatible => PackStatus::Incompatible,
        }
    }

    /// ¿Se puede intentar jugar? Sólo `Incompatible` dice que no — igual que
    /// `CompatGrade::runnable`, y por el mismo motivo: «no verificado» es «nadie
    /// lo comprobó», no «no anda».
    pub fn is_playable(self) -> bool {
        self != PackStatus::Incompatible
    }
}

/// El id de plataforma que usa el FORMATO (`compat.platform` del manifest),
/// desde el nombre que muestra Play.
///
/// Son dos vocabularios y hay que traducir: la biblioteca dice «Mega Drive»
/// porque es lo que el jugador lee, y el pack dice «megadrive» porque es un
/// identificador. Sin esta tabla el validador recibe `None` y **todo pack queda
/// «no verificado»**, que es peor que un error: se lee como que el semáforo no
/// funciona.
pub fn platform_id(name: &str) -> Option<&'static str> {
    Some(match name {
        "Mega Drive" => "megadrive",
        "Master System" => "mastersystem",
        "SNES" => "snes",
        "NES" => "nes",
        "GBA" => "gba",
        "Game Boy" => "gameboy",
        "GBC" => "gbc",
        "N64" => "n64",
        "PC Engine" => "pcengine",
        _ => return None,
    })
}

impl Game {
    /// El título que se muestra: el del jugador si lo hay, el automático si no.
    ///
    /// Un override en blanco NO cuenta: es alguien que borró el campo del
    /// diálogo, y guardarlo dejaría una tarjeta sin nombre y sin forma evidente
    /// de recuperarlo.
    pub fn title(&self) -> String {
        match self.user_title.as_deref().map(str::trim) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => self.name.clone(),
        }
    }

    /// La carátula que se muestra, con el mismo criterio.
    pub fn artwork(&self) -> String {
        match self.user_art.as_deref().map(str::trim) {
            Some(a) if !a.is_empty() => a.to_string(),
            _ => self.cover_path.clone(),
        }
    }

    /// ¿Hay algo editado a mano que «restaurar valores automáticos» pueda
    /// deshacer? La UI lo usa para no ofrecer una acción que no haría nada.
    pub fn has_edits(&self) -> bool {
        self.user_title.is_some() || self.user_art.is_some()
    }

    /// #593: lo que decidió el JUGADOR viaja de la entrada vieja a la nueva.
    ///
    /// Es la lista de lo que el disco NO sabe, y está en UN solo lugar a
    /// propósito: cada campo nuevo de esta clase se agrega acá y todos los
    /// caminos que fusionan lo heredan solos. La alternativa —copiar campo por
    /// campo en cada sitio— es una lista repetida, y la que se olvide de
    /// actualizar borra datos del jugador sin decir nada.
    fn keep_from_player(&mut self, previous_game: &Game) {
        self.favorite = previous_game.favorite;
        self.hidden = previous_game.hidden;
        self.user_title = previous_game.user_title.clone();
        self.user_art = previous_game.user_art.clone();
        self.last_played = previous_game.last_played;
        self.seconds_played = previous_game.seconds_played;
    }

    /// #591: la misma ROM, en otra carpeta. Se actualiza sólo lo que DEPENDE de
    /// dónde está el archivo; todo lo demás —lo que el escaneo no produce— es
    /// justamente lo que hay que conservar, y se conserva por no tocarlo.
    ///
    /// `name` y `region` no se copian del escaneo: los pone el DAT (o el
    /// jugador, a partir de #593) y el escaneo sólo sabe el nombre del archivo.
    fn update_location_from(&mut self, new: &Game) {
        self.path = new.path.clone();
        self.platform = new.platform.clone();
        self.has_pack = new.has_pack;
        self.pack_path = new.pack_path.clone();
        self.pack_status = new.pack_status;
        self.available_packs.clone_from(&new.available_packs);
        self.compat_reason.clone_from(&new.compat_reason);
        self.compat_codes.clone_from(&new.compat_codes);
        self.cover_path = new.cover_path.clone();
        self.size_bytes = new.size_bytes;
        self.mtime = new.mtime;
    }
}

/// A single game entry in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    /// Display name.  Set to the filename stem during scan; overwritten by
    /// the No-Intro DAT name if a DAT is configured.
    pub name: String,
    /// Absolute path to the ROM file.
    pub path: String,
    /// Human-readable platform name (e.g. "Mega Drive").
    pub platform: String,
    /// 8-char uppercase hex CRC32 (empty if hashing failed or file is too large).
    pub crc32: String,
    /// Two-letter region code from No-Intro (e.g. "WLD", "USA").  "UNK" if
    /// the CRC32 was not found in the DAT, "" if no DAT is loaded.
    pub region: String,
    /// True if a `.ay` HD pack exists next to this ROM.
    pub has_pack: bool,
    /// Absolute path to the `.ay` pack (empty if none).
    pub pack_path: String,
    /// Readiness of the HD pack (see `PackStatus`).
    pub pack_status: PackStatus,
    /// Path to the cover art image on disk (empty = not found).
    /// `slint::Image` is !Send so the cover is loaded by `load_covers()` on
    /// the Slint event-loop thread, not here.
    pub cover_path: String,
    /// #595: POR QUÉ el pack tiene ese grado, en palabras del validador.
    ///
    /// Va en el modelo y no se re-deriva al pintar porque el grado sin el motivo
    /// obliga al que lo muestra a inventar el texto — y ahí cada pantalla dice
    /// una cosa distinta sobre el mismo pack. Es lo que hace accionable al
    /// semáforo: «incompatible» no ayuda, «el pack se horneó para otra ROM» sí.
    #[serde(default)]
    #[serde(alias = "compat_motivo")]
    pub compat_reason: String,

    /// #597: los CÓDIGOS de los hallazgos de compatibilidad, para los tres ejes
    /// de la ficha.
    ///
    /// Se guardan acá porque el escaneo YA corrió el validador para sacar el
    /// grado: tirar el informe y volver a abrir el pack al abrir la ficha
    /// costaría —medido— varios segundos en un `.ay` de 138 MB, **en el hilo de
    /// la UI**, que es como una ficha se convierte en una ventana congelada.
    /// Son media docena de cadenas cortas por juego.
    #[serde(default)]
    #[serde(alias = "compat_codigos")]
    pub compat_codes: Vec<String>,

    /// #591: tamaño del archivo, en bytes, cuando se lo hasheó.
    ///
    /// Junto con `mtime` es lo que hace INCREMENTAL al escaneo: si los dos
    /// coinciden con lo que la caché ya tiene, el CRC de esa ROM no cambió y no
    /// hay por qué volver a leerla entera. Sin esto, cada re-escaneo re-hashea
    /// la biblioteca completa — con el tope de 64 MiB puesto eso se notaba
    /// poco, y sin el tope pasa a ser el costo dominante.
    ///
    /// `serde(default)` porque toda caché escrita antes de #591 no los trae: se
    /// leen como 0, que no coincide con ningún archivo real, así que esas
    /// entradas se re-hashean UNA vez y quedan al día solas.
    #[serde(default)]
    pub size_bytes: u64,
    /// #591: fecha de modificación (segundos desde epoch) cuando se lo hasheó.
    #[serde(default)]
    pub mtime: i64,

    /// #634: TODOS los packs que este juego tiene disponibles, en orden de
    /// precedencia. `pack_path` es el seleccionado, o el primero por defecto.
    ///
    /// Son dos y no uno porque hay dos lugares de donde puede venir un pack: al
    /// lado de la ROM —el que el jugador copió a mano— y el directorio de
    /// instalados, donde deja los suyos el Hub. Hasta #634 la biblioteca sólo
    /// miraba el primero, así que un pack bajado, verificado e instalado no
    /// llegaba nunca a su juego.
    ///
    /// La lista existe desde ya, con la elección todavía cerrada (gana el
    /// primero), porque el modelo tiene que admitir varios antes de que la
    /// pantalla deje elegir (#633). Guardarlo como un solo campo obligaría a
    /// migrar la caché de nuevo cuando esa elección llegue.
    #[serde(default)]
    #[serde(alias = "packs_disponibles")]
    pub available_packs: Vec<String>,

    // -- #593: lo que decide el JUGADOR --------------------------------------
    //
    // Todo lo de acá abajo es suyo y NO se re-deriva del disco. Vive en la misma
    // entrada que lo automático —no en un archivo aparte— porque así la
    // reconciliación por hash de #591 lo conserva al mover una ROM sin que nadie
    // tenga que acordarse de copiarlo.
    /// Marcado como favorito. Los favoritos encabezan cualquier orden.
    #[serde(default)]
    #[serde(alias = "favorito")]
    pub favorite: bool,

    /// Fuera de la vista. **No es borrar**: la entrada queda entera —favorito,
    /// título editado, tiempo jugado— y volver a mostrarla la devuelve como
    /// estaba. Borrarla perdería todo eso y el próximo escaneo la traería de
    /// vuelta igual, así que además no serviría.
    #[serde(default)]
    #[serde(alias = "oculto")]
    pub hidden: bool,

    /// El título que escribió el jugador, si escribió alguno.
    ///
    /// Es un campo PROPIO y no una escritura sobre `name` porque `name` lo pone
    /// el DAT en cada escaneo: pisarlo haría que el próximo re-escaneo borrara
    /// la edición, y no pisarlo pero tampoco guardarlo aparte dejaría sin manera
    /// de volver a lo automático. Con los dos separados, «restaurar valores
    /// automáticos» es vaciar este campo — no re-deriva nada y no puede fallar.
    #[serde(default)]
    #[serde(alias = "titulo_usuario")]
    pub user_title: Option<String>,

    /// Ídem para la carátula: la ruta que el jugador eligió a mano, contra la
    /// que `find_covers` encuentra sola.
    #[serde(default)]
    #[serde(alias = "arte_usuario")]
    pub user_art: Option<String>,

    /// #594: última vez que se jugó (segundos desde epoch; 0 = nunca) y cuánto
    /// se jugó en total. Se declaran acá y no en #594 porque el orden «por
    /// recientes» de #593 los necesita para existir, y un orden que ordena por
    /// un campo que siempre vale 0 se ve como aleatorio en vez de como vacío.
    #[serde(default)]
    #[serde(alias = "ultima_vez")]
    pub last_played: i64,
    #[serde(default)]
    #[serde(alias = "segundos_jugados")]
    pub seconds_played: u64,
}

// ---------------------------------------------------------------------------
// Platform detection
// ---------------------------------------------------------------------------

pub(crate) fn platform_for_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "md" | "gen" => Some("Mega Drive"),
        "bin" | "rom" => Some("Mega Drive"),
        "smc" | "sfc" | "fig" => Some("SNES"),
        "nes" | "unf" => Some("NES"),
        "gba" => Some("GBA"),
        "n64" | "z64" | "v64" => Some("N64"),
        "gb" => Some("Game Boy"),
        "gbc" => Some("GBC"),
        "pce" => Some("PC Engine"),
        "sms" | "gg" => Some("Master System"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Scanner (rayon phase — no Slint types)
// ---------------------------------------------------------------------------

/// El resultado de un escaneo, y si llegó hasta el final.
///
/// `cancelado` viaja con los juegos en vez de reemplazarlos porque un escaneo
/// cortado a mitad SÍ produjo resultados válidos —los archivos que alcanzó a
/// mirar— y tirarlos sería re-hashearlos la próxima vez sin ninguna necesidad.
/// Quien recibe esto decide qué hacer con el estado, no si los datos sirven.
pub struct ScanOutcome {
    pub games: Vec<Game>,
    pub cancelled: bool,
}

/// El escaneo de verdad: incremental, cancelable y con progreso.
///
/// `conocidos` es la caché vigente indexada por ruta. Una ROM cuyo tamaño y
/// fecha coinciden con los de ahí NO se vuelve a leer: se reusa su CRC. Es la
/// diferencia entre re-escanear una biblioteca de 200 ROMs en un segundo o en
/// varios minutos, y sin ella quitar el tope de 64 MiB habría empeorado el
/// re-escaneo en vez de arreglarlo.
///
/// `cancel` se consulta por archivo. La granularidad es ésa y no menor a
/// propósito: cortar a mitad de un hash dejaría un CRC parcial que se ve igual
/// que uno bueno, y ése es justo el tipo de dato corrupto que la caché no puede
/// distinguir después.
///
/// `progreso` recibe `(hechos, total)` y se llama desde los hilos de rayon, así
/// que tiene que ser `Sync` y barato — quien quiera tocar la UI que empaquete y
/// se lo mande al hilo del event loop.
pub fn scan_incremental(
    dir: &Path,
    known_games: &HashMap<String, Game>,
    cancel: &CancelToken,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> ScanOutcome {
    crate::log_info!("library", "escaneando {}", dir.display());

    let paths: Vec<PathBuf> = WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .and_then(platform_for_ext)
                .is_some()
        })
        .map(|e| e.into_path())
        .collect();

    let total = paths.len();
    eprintln!("[library] {} ROM candidates", total);
    progress(0, total);

    let done = AtomicUsize::new(0);
    let mut games: Vec<Game> = paths
        .par_iter()
        .filter_map(|p| {
            // El chequeo va ANTES de tocar el disco: cancelar tiene que dejar
            // de trabajar, no terminar el trabajo en curso más rápido.
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let g = scan_one(p, known_games);
            progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            g
        })
        .collect();
    games.sort_by(|a, b| a.name.cmp(&b.name));

    let cancelled = cancel.load(Ordering::Relaxed);
    eprintln!(
        "[library] Scan {}: {} ROMs",
        if cancelled { "cancelado" } else { "done" },
        games.len()
    );
    ScanOutcome { games, cancelled }
}

fn scan_one(path: &Path, known_games: &HashMap<String, Game>) -> Option<Game> {
    let ext = path.extension()?.to_str()?;
    let platform = platform_for_ext(ext)?.to_string();
    let name = path.file_stem()?.to_str()?.to_owned();

    let (size_bytes, mtime) = fingerprint(path);
    // #591: el atajo incremental. Mismo tamaño Y misma fecha = mismo contenido,
    // así que el CRC guardado sigue valiendo y el archivo no se abre. Se exigen
    // los dos: el tamaño solo deja pasar una edición que no cambió el largo, y
    // la fecha sola deja pasar una copia con timestamp preservado.
    //
    // El 0 no entra en el atajo (`huella` devuelve 0 si no pudo leer los
    // metadatos, y toda caché anterior a #591 trae 0): ante la duda, se hashea.
    let cached_game = known_games
        .get(path.to_string_lossy().as_ref())
        .filter(|g| size_bytes != 0 && g.size_bytes == size_bytes && g.mtime == mtime)
        .filter(|g| !g.crc32.is_empty());

    let crc32 = match cached_game {
        Some(g) => g.crc32.clone(),
        None => compute_crc32(path).unwrap_or_default(),
    };

    // Pack junto a la ROM: .ay es el formato actual; .ae queda como legacy
    // pre-rebrand (se sigue leyendo, nunca se escribe).
    let mut pack_path = path.with_extension("ay");
    if !pack_path.exists() {
        let legacy = path.with_extension("ae");
        if legacy.exists() {
            pack_path = legacy;
        }
    }
    let has_pack = pack_path.exists();

    Some(Game {
        name,
        path: path.to_string_lossy().into_owned(),
        platform,
        crc32,
        region: String::new(), // filled by enrich_with_dat()
        has_pack,
        pack_path: if has_pack {
            pack_path.to_string_lossy().into_owned()
        } else {
            String::new()
        },
        // #634: los INSTALADOS los cuelga `asociar_packs_instalados`, que corre
        // después. Esta pasada mira un archivo suelto y no conoce el directorio
        // de packs ni le corresponde abrirlos.
        available_packs: if has_pack {
            vec![pack_path.to_string_lossy().into_owned()]
        } else {
            Vec::new()
        },
        // #595: el escaneo NO gradúa. Graduar exige el CRC de la ROM, la
        // plataforma y el core, y esta pasada corre en paralelo sobre archivos
        // sueltos. Lo hace `enrich_pack_status`, que sí tiene el contexto.
        pack_status: if has_pack {
            PackStatus::Unverified
        } else {
            PackStatus::NoPack
        },
        compat_reason: String::new(),
        compat_codes: Vec::new(),
        cover_path: String::new(), // filled by find_covers()
        size_bytes,
        mtime,
        // #593: lo del jugador NO lo produce el escaneo. Un escaneo que los
        // rellenara borraría sus favoritos cada vez; `upsert_games` conserva la
        // entrada vieja para las rutas conocidas y la re-ubica para las movidas,
        // así que estos valores sólo se usan cuando el juego es NUEVO de verdad.
        favorite: false,
        hidden: false,
        user_title: None,
        user_art: None,
        last_played: 0,
        seconds_played: 0,
    })
}

/// Tamaño y fecha de modificación, o `(0, 0)` si no se pudieron leer. El cero
/// es deliberadamente un valor que ningún archivo real tiene como huella
/// completa, así que se lee como «no sé» y manda a hashear.
fn fingerprint(path: &Path) -> (u64, i64) {
    let Ok(md) = path.metadata() else {
        return (0, 0);
    };
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (md.len(), mtime)
}

/// CRC32 del archivo, leído POR PARTES.
///
/// Antes esto era `std::fs::read()` —el archivo entero en RAM— y por eso existía
/// un tope de 64 MiB más arriba que dejaba a las ROMs grandes con el CRC vacío.
/// Un CRC vacío no falla: la ROM entra a la biblioteca sin match en el DAT, o
/// sea sin nombre canónico y sin región, y el jugador ve un nombre de archivo
/// donde debería ver el título del juego. El tope no se subió, se RETIRÓ: con
/// un buffer fijo el tamaño del archivo dejó de ser un problema de memoria.
fn compute_crc32(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:08X}", hasher.finalize()))
}

// ---------------------------------------------------------------------------
// DAT enrichment (main thread, after scan)
// ---------------------------------------------------------------------------

use crate::nointro::DatFile;

/// Overwrite `game.name` and `game.region` for every entry whose CRC32 is
/// found in `dat`.  Entries not in the DAT keep their filename-derived name
/// and get `region = "UNK"`.
pub fn enrich_with_dat(games: &mut [Game], dat: &DatFile) {
    for g in games.iter_mut() {
        if g.crc32.is_empty() {
            continue;
        }
        if let Some(entry) = dat.lookup(&g.crc32) {
            g.name = entry.name.clone();
            g.region = entry.region.clone();
        } else {
            g.region = "UNK".to_string();
        }
    }
    // Re-sort by name after enrichment (DAT names differ from filenames).
    games.sort_by(|a, b| a.name.cmp(&b.name));
}

// ---------------------------------------------------------------------------
// #634 — los packs INSTALADOS, y a qué juego pertenece cada uno
// ---------------------------------------------------------------------------
//
// EL HUECO QUE ESTO CIERRA. El escaneo reconoce un pack sólo si está **al lado
// de la ROM** (`scan_one`, más arriba: `path.with_extension("ay")`), y la
// instalación del Hub deja el suyo en `packs_install_dir/<id>.ay`
// (`pack_install.rs`). Los dos árboles no se cruzaban en ningún punto: se podía
// descargar un pack, verificar su firma e instalarlo, y la ficha del juego
// seguía diciendo «No hay pack para este juego — se juega el original». Todo el
// trabajo de distribución (#613, #614, #616, #617) desembocaba en un pack que
// el jugador no podía usar.
//
// EL VÍNCULO SALE DEL PACK, NO DEL NOMBRE DEL ARCHIVO. El manifest declara
// `compat.rom_crc32` —el CRC32 de la ROM para la que se horneó— y ése es el
// mismo dato con el que el validador da su veredicto (#530/#595). Emparejar por
// nombre sería más barato y estaría mal de las dos maneras posibles: un pack
// renombrado dejaría de encontrar su juego, y un `.ay` que se llama como la ROM
// pero es de otra se asociaría igual. El nombre del archivo es del jugador; el
// CRC es del contenido.

/// Un pack instalado, con lo poco que hace falta para colocarlo.
#[derive(Debug, Clone)]
pub struct InstalledPack {
    /// El id con el que se instaló, que es el nombre del archivo sin extensión.
    pub id: String,
    pub path: String,
    /// El CRC32 de la ROM para la que se horneó, en hex minúscula. `None` = el
    /// pack no lo declara, y entonces no hay a qué juego atarlo.
    pub rom_crc32: Option<String>,
}

/// Lee el directorio de packs instalados y saca de cada uno a qué ROM apunta.
///
/// Abre cada `.ay` UNA vez POR ESCANEO, y no una vez por ficha pintada, que era
/// lo que había que evitar: son los packs que el jugador instaló —unos pocos, no
/// la biblioteca entera— y de cada uno se lee el manifest, no el contenido. El
/// lector del core es perezoso (#332): abrir un pack es leer el directorio del
/// ZIP y su manifest, no los cientos de MB de assets.
///
/// No hay caché por `mtime` como la del CRC de las ROMs. Sería la misma
/// mecánica y no la paga: el escaneo ya recorre la biblioteca entera hasheando
/// lo que cambió, y esto son unas pocas aperturas baratas al final. Una caché
/// más es una caché más que puede quedar rancia. Los archivos de trabajo del instalador (`.new`, `.prev`, `.part`) no
/// son packs instalados y se ignoran: `.prev` es la generación anterior que
/// `rollback` guarda, y ofrecerla como si fuera un pack más sería ofrecer dos
/// veces el mismo.
pub fn index_installed_packs(dir: &Path) -> Vec<InstalledPack> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<InstalledPack> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ay"))
        .filter_map(|p| {
            let path = p.to_string_lossy().to_string();
            let arch = ayther_core::archive_vfs::AyArchive::open_verbose(&path).ok()?;
            Some(InstalledPack {
                id: p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path,
                rom_crc32: declared_crc(&arch),
            })
        })
        .collect();
    // Orden estable por id: con dos packs para el mismo juego, cuál gana no
    // puede depender de cómo el sistema de archivos devolvió el directorio.
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Los packs de TODAS las carpetas que el jugador mira, en orden de precedencia
/// (#678): primero los instalados, después su carpeta de packs.
///
/// El orden de `dirs` ES la precedencia — `asociar_packs_instalados` cuelga las
/// rutas en el orden en que llegan y la primera gana. Instalar es un acto
/// explícito («este pack quiero tenerlo puesto»), así que un `.ay` que apareció
/// en la carpeta no puede desplazarlo; al revés sí tiene sentido: instalar es
/// justamente elegir.
///
/// Cada carpeta se lee plana, sin recursión, como `indexar_packs_instalados`:
/// la carpeta de packs es una colección que el jugador arma, no un árbol que
/// haya que rastrillar.
pub fn index_packs(dirs: &[std::path::PathBuf]) -> Vec<InstalledPack> {
    let mut v = Vec::new();
    for d in dirs {
        v.extend(index_installed_packs(d));
    }
    v
}

/// El CRC de la ROM que declara un pack, normalizado a hex minúscula.
///
/// Primero el campo, que es lo que un validador puede comparar sin parsear
/// cadenas; y si no está, el `game_id` con la convención `crc32:7b905383`, que
/// es como lo escriben los packs horneados antes de que `[compat]` existiera
/// (#294). Un pack que no dice ninguna de las dos cosas no se ata a ningún
/// juego: es preferible que no aparezca a que aparezca en el equivocado.
fn declared_crc(arch: &ayther_core::archive_vfs::AyArchive) -> Option<String> {
    if let Some(c) = arch.compat.rom_crc32.as_deref() {
        let c = c.trim().to_lowercase();
        if !c.is_empty() {
            return Some(c);
        }
    }
    arch.meta
        .game_id
        .strip_prefix("crc32:")
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty())
}

/// Cuelga de cada juego los packs instalados que le corresponden (#634).
///
/// El pack que está AL LADO DE LA ROM va primero y sigue ganando. Es el que el
/// jugador puso a mano, así que es el que espera ver; que un pack instalado lo
/// desplazara convertiría en no determinista algo que hoy es explícito, y del
/// peor modo: sin que nada en pantalla explique por qué cambió.
///
/// Un juego sin CRC —una ROM que no se pudo hashear— no empareja con nada. No es
/// un caso a resolver con el nombre: sin el dato del contenido, cualquier
/// coincidencia sería una adivinanza.
pub fn attach_installed_packs(games: &mut [Game], packs: &[InstalledPack]) {
    for g in games.iter_mut() {
        let mut paths: Vec<String> = Vec::new();
        // 1) El de al lado de la ROM, si lo hay.
        if g.has_pack && !g.pack_path.is_empty() {
            paths.push(g.pack_path.clone());
        }
        // 2) Los instalados que declaran ESTA ROM.
        let crc = g.crc32.trim().to_lowercase();
        if !crc.is_empty() {
            for p in packs {
                if p.rom_crc32.as_deref() == Some(crc.as_str())
                    && !paths.iter().any(|r| r == &p.path)
                {
                    paths.push(p.path.clone());
                }
            }
        }
        g.has_pack = !paths.is_empty();
        g.pack_path = paths.first().cloned().unwrap_or_default();
        g.available_packs = paths;
    }
}

// ---------------------------------------------------------------------------
// Pack status enrichment (main thread, after scan)
// ---------------------------------------------------------------------------

/// Read each game's `.ay` pack manifest and update `pack_status`.
/// Games without a pack are left unchanged (PackStatus::None).
/// #595 — el grado de compatibilidad de cada pack, contra ESTA sesión.
///
/// Se lo pregunta a `ayther_core::pack_validate::compat_grade`, que es la única
/// implementación de la matriz de cuatro grados (#530): el mismo veredicto que
/// le dio `ay_pack validate` al autor y que le va a dar el Hub al recibirlo. Lo
/// que había antes era un criterio propio —«¿se puede leer el manifest?»—, así
/// que Play decía «Ok» de un pack que el validador rechazaba, y `Mismatch` no se
/// asignaba NUNCA aunque el pack fuera de otro juego.
///
/// `core_build_id` es lo que separa «compatible» de «no verificado»: sin él
/// `compat_grade` no puede cerrar el caso y contesta `Experimental`, que es la
/// respuesta honesta. Lo aporta el sondeo del core (#589), cacheado una vez por
/// sesión — sondear por juego sería lanzar un proceso por tarjeta.
pub fn enrich_pack_status(games: &mut [Game], core_build_id: Option<&str>) {
    enrich_pack_status_progress(games, core_build_id, &|_, _| {});
}

/// Report checked entries. Complete their grades before publishing a partial scan.
pub fn enrich_pack_status_progress(
    games: &mut [Game],
    core_build_id: Option<&str>,
    progress: &dyn Fn(usize, usize),
) {
    use ayther_core::pack_validate::{SessionCtx, compat_grade};
    let total = games.len();
    progress(0, total);
    for (index, g) in games.iter_mut().enumerate() {
        if !g.has_pack || g.pack_path.is_empty() {
            g.pack_status = PackStatus::NoPack;
            g.compat_reason = String::new();
            g.compat_codes.clear();
            progress(index + 1, total);
            continue;
        }
        let ctx = SessionCtx {
            rom_crc32: u32::from_str_radix(&g.crc32, 16).ok(),
            platform: platform_id(&g.platform),
            core_build_id,
            ..Default::default()
        };
        let v = compat_grade(&g.pack_path, &ctx);
        g.pack_status = PackStatus::from_grade(v.grade);
        g.compat_reason = v.reason;
        // Sólo los de COMPATIBILIDAD: los de publicación (licencia, autor,
        // procedencia) no dicen nada sobre si el juego corre acá, y meterlos
        // haría que la ficha mostrara problemas del autor como problemas del
        // jugador.
        g.compat_codes = v
            .report
            .findings
            .iter()
            .filter(|f| ayther_core::pack_validate::affects_compatibility(f.code))
            .map(|f| f.code.to_string())
            .collect();
        progress(index + 1, total);
    }
}

// ---------------------------------------------------------------------------
// Cover art discovery (main thread, after scan)
// ---------------------------------------------------------------------------

/// Discover a cover art image path for each game.  Checked in this order:
///
///   1. Image beside the ROM with the same file stem (PNG, JPG, JPEG, WebP).
///   2. `{covers_dir}/{CRC32}.{ext}` in the same format order.
///   3. `{covers_dir}/{slug}.{ext}` (slug = sanitized lowercase name).
///
/// Sets `game.cover_path` to the first match found (empty string if none).
/// Manual choices remain separate in `user_art` and take precedence in `artwork()`.
pub fn find_covers(games: &mut [Game], covers_dir: &Path) {
    for g in games.iter_mut() {
        g.cover_path = find_cover_path(g, covers_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
}

fn find_cover_path(game: &Game, dir: &Path) -> Option<PathBuf> {
    let rom = Path::new(&game.path);
    if let (Some(parent), Some(stem)) = (rom.parent(), rom.file_stem())
        && let Ok(entries) = std::fs::read_dir(parent)
    {
        // Directory enumeration order is unspecified. Rank formats, then paths,
        // so rescans cannot randomly change the selected cover.
        let adjacent = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.file_stem() != Some(stem) || !path.is_file() {
                    return None;
                }
                let ext = path.extension()?.to_str()?;
                let rank = ["png", "jpg", "jpeg", "webp"]
                    .iter()
                    .position(|candidate| ext.eq_ignore_ascii_case(candidate))?;
                Some((rank, path))
            })
            .min();
        if let Some((_, path)) = adjacent {
            return Some(path);
        }
    }
    // By CRC32
    if !game.crc32.is_empty() {
        for ext in &["png", "jpg", "jpeg", "webp"] {
            let p = dir.join(format!("{}.{}", game.crc32, ext));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // By slug name
    let slug = game
        .name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "_");
    for ext in &["png", "jpg", "jpeg", "webp"] {
        let p = dir.join(format!("{slug}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Library — JSON-backed cache
// ---------------------------------------------------------------------------

#[derive(Default, Serialize, Deserialize)]
struct LibraryData {
    games: HashMap<String, Game>,
}

pub struct Library {
    path: PathBuf,
    data: LibraryData,
}

impl Library {
    pub fn open(path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data = if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            LibraryData::default()
        };
        Library {
            path: path.to_owned(),
            data,
        }
    }

    /// Las entradas vigentes, por ruta — lo que `scan_incremental` necesita
    /// para saltear lo que no cambió.
    pub fn known_games(&self) -> &HashMap<String, Game> {
        &self.data.games
    }

    /// Restore selection after discovery and before grading the selected pack.
    pub fn restore_pack_selection(&self, games: &mut [Game]) {
        for game in games {
            let previous = self.data.games.get(&game.path).or_else(|| {
                self.data.games.values().find(|old| {
                    !game.crc32.is_empty()
                        && old.crc32 == game.crc32
                        && !Path::new(&old.path).exists()
                })
            });
            if let Some(previous) = previous
                && game.available_packs.contains(&previous.pack_path)
            {
                game.pack_path.clone_from(&previous.pack_path);
            }
        }
    }

    /// Merge scan results and persist.
    ///
    /// #591 — LO MOVIDO SE RECONOCE POR HASH. Indexar sólo por ruta hacía que
    /// mover una ROM de carpeta la convirtiera en una entrada NUEVA: la vieja
    /// quedaba huérfana hasta el próximo `prune_missing` y todo lo que colgara
    /// de ella se perdía en silencio. Hoy eso todavía no duele porque no hay
    /// nada que el jugador edite; a partir de #593 hay favoritos y metadatos, y
    /// entonces mover una carpeta borraría su trabajo.
    ///
    /// Cuando el CRC coincide con una entrada cuya ruta ya no existe, se
    /// MODIFICA la entrada vieja en vez de insertar una nueva: así lo que el
    /// escaneo no sabe re-derivar sobrevive **sin que nadie tenga que
    /// enumerarlo**. Al revés —copiar campo por campo de la vieja a la nueva—
    /// hay una lista que actualizar cada vez que el modelo crece, y es la clase
    /// de lista que se olvida.
    pub fn upsert_games(&mut self, games: &[Game]) {
        for g in games {
            if let Some(previous_game) = self.data.games.get(&g.path) {
                // #593 — SE FUSIONA, no se reemplaza. Reemplazar estuvo bien
                // mientras `Game` era puro dato del disco; desde que hay
                // favoritos, títulos editados y tiempo de juego, un escaneo
                // reemplazando la entrada BORRA todo eso — y no ruidosamente:
                // el juego sigue ahí, sólo que ya no es favorito y volvió a
                // llamarse como el archivo. Se descubrió apretando F5 con un
                // favorito puesto.
                let mut new = g.clone();
                new.keep_from_player(previous_game);
                self.data.games.insert(g.path.clone(), new);
                continue;
            }
            // Ruta nueva: ¿es una ROM movida? Sólo cuenta si la entrada vieja
            // apunta a un archivo que ya NO está — si las dos existen son dos
            // copias del mismo juego, y son dos entradas legítimas.
            let previous_path = (!g.crc32.is_empty())
                .then(|| {
                    self.data
                        .games
                        .iter()
                        .find(|(path, previous_game)| {
                            previous_game.crc32 == g.crc32
                                && path.as_str() != g.path
                                && !Path::new(path.as_str()).exists()
                        })
                        .map(|(path, _)| path.clone())
                })
                .flatten();

            match previous_path {
                Some(old_path) => {
                    let mut entry = self
                        .data
                        .games
                        .remove(&old_path)
                        .expect("la ruta salió de este mismo mapa");
                    entry.update_location_from(g);
                    self.data.games.insert(g.path.clone(), entry);
                }
                None => {
                    self.data.games.insert(g.path.clone(), g.clone());
                }
            }
        }
        self.save();
    }

    /// #593: guarda UNA entrada tal cual, para el camino de las ediciones.
    ///
    /// Es una operación distinta de `upsert_games` y por eso es otra función.
    /// Aquélla viene del escaneo, donde lo del jugador hay que CONSERVARLO
    /// porque el disco no lo sabe; ésta viene de que el jugador acaba de
    /// cambiarlo, así que conservarlo sería deshacer lo que pidió. Una sola
    /// función con una bandera habría hecho que el día que alguien pase la
    /// bandera al revés el síntoma sea «marcar favorito no hace nada».
    pub fn save_game(&mut self, g: &Game) {
        self.data.games.insert(g.path.clone(), g.clone());
        self.save();
    }

    /// #594: anota que se jugó a la ROM de `path` durante `segundos`.
    ///
    /// Va acá y no en el llamador porque la sesión termina en OTRO hilo —el que
    /// espera al Runtime— y lo único que ese hilo tiene a mano es la ruta. Que
    /// la biblioteca sepa acumular evita que el fin de sesión tenga que leer la
    /// entrada, sumar y volver a guardarla, que es donde se pierde el registro
    /// si dos sesiones terminan cerca.
    ///
    /// `ultima_vez` se escribe SIEMPRE que la ruta exista, aunque la sesión haya
    /// durado dos segundos: haber abierto un juego es información —es lo que
    /// «Continuar» necesita— incluso si no alcanzó a jugarse.
    ///
    /// Devuelve `false` si la ROM no está en la biblioteca, que pasa cuando se
    /// lanzó algo que después se borró del disco: no es un error, es que no hay
    /// dónde anotarlo.
    pub fn record_session(&mut self, path: &str, seconds: u64, when: i64) -> bool {
        let Some(g) = self.data.games.get_mut(path) else {
            return false;
        };
        g.last_played = when;
        g.seconds_played = g.seconds_played.saturating_add(seconds);
        self.save();
        true
    }

    /// Remove entries whose ROM no longer exists and persist.
    pub fn prune_missing(&mut self) -> usize {
        let before = self.data.games.len();
        self.data.games.retain(|path, _| Path::new(path).exists());
        let removed = before - self.data.games.len();
        if removed > 0 {
            self.save();
        }
        removed
    }

    /// All games sorted by name.
    pub fn get_all(&self) -> Vec<Game> {
        let mut games: Vec<Game> = self.data.games.values().cloned().collect();
        games.sort_by(|a, b| a.name.cmp(&b.name));
        games
    }

    /// Persiste la caché de forma ATÓMICA: se escribe a un temporal al lado y
    /// recién ahí se renombra sobre el bueno.
    ///
    /// #591 — es lo que hace cierto el criterio «cancelar deja la caché
    /// consistente». Con `fs::write` directo, cortar el proceso en el medio
    /// —cancelar, cerrar Play, quedarse sin batería— dejaba un JSON truncado, y
    /// un JSON truncado no parsea: `Library::open` cae a `unwrap_or_default()` y
    /// el jugador arranca con la biblioteca VACÍA. El rename es lo único que el
    /// sistema de archivos garantiza que pasa o no pasa.
    fn save(&self) {
        let Ok(json) = serde_json::to_string_pretty(&self.data) else {
            return;
        };
        // El temporal va en el MISMO directorio que el destino: un rename entre
        // volúmenes no es atómico (ni siquiera es un rename), y el temp del
        // sistema puede estar en otro disco.
        let tmp = self.path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, json) {
            eprintln!("[library] Failed to save: {e}");
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            eprintln!("[library] Failed to commit: {e}");
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_covers_work_without_a_covers_directory_and_keep_manual_choices() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "Game (USA).md", b"rom");
        let image = dir.path().join("Game (USA).JPG");
        std::fs::write(&image, b"image").unwrap();
        let mut games = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        games[0].user_art = Some("manual.webp".into());
        find_covers(&mut games, &dir.path().join("missing"));
        assert_eq!(games[0].cover_path, image.to_string_lossy());
        assert_eq!(games[0].artwork(), "manual.webp");
        games[0].user_art = None;
        assert_eq!(games[0].artwork(), image.to_string_lossy());
    }

    #[test]
    fn adjacent_covers_use_a_stable_format_order_before_the_covers_directory() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "Game.md", b"rom");
        let game = scan_one(&rom, &HashMap::new()).unwrap();
        let covers = dir.path().join("covers");
        std::fs::create_dir(&covers).unwrap();
        std::fs::write(covers.join(format!("{}.png", game.crc32)), b"image").unwrap();
        for ext in ["webp", "jpeg", "jpg", "png"] {
            std::fs::write(rom.with_extension(ext), b"image").unwrap();
        }
        assert_eq!(
            find_cover_path(&game, &covers),
            Some(rom.with_extension("png"))
        );
        std::fs::remove_file(rom.with_extension("png")).unwrap();
        assert_eq!(
            find_cover_path(&game, &covers),
            Some(rom.with_extension("jpg"))
        );
    }

    #[test]
    fn adjacent_cover_discovery_ignores_directories_and_clears_removed_images() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "Game.md", b"rom");
        std::fs::create_dir(rom.with_extension("png")).unwrap();
        std::fs::write(dir.path().join("Other.jpg"), b"image").unwrap();
        let mut games = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        games[0].cover_path = "deleted.png".into();
        find_covers(&mut games, &dir.path().join("missing"));
        assert!(games[0].cover_path.is_empty());
    }

    /// La biblioteca resuelve la ruta del pack UNA vez y la guarda. Que esa
    /// ruta viaje en el modelo —en vez de re-derivarse donde se necesita— es lo
    /// que evita el defecto que tuvo la pantalla previa: derivaba
    /// `rom.with_extension("ae")` a mano, así que sobre un pack `.ay` (todo
    /// pack horneado desde el rebranding) abría una ruta inexistente y la
    /// pantalla salía sin capacidades, sin perfiles y sin créditos, en silencio.
    #[test]
    fn the_pack_resolves_to_ay_and_falls_back_to_legacy_ae() {
        let dir = tempfile::tempdir().unwrap();
        let rom = dir.path().join("juego.md");
        std::fs::write(&rom, b"rom").unwrap();

        // Sin pack al lado.
        let g = scan_one(&rom, &HashMap::new()).unwrap();
        assert!(!g.has_pack);
        assert_eq!(g.pack_path, "");

        // Sólo el legacy .ae: se lee igual (nunca se escribe).
        let ae = dir.path().join("juego.ae");
        std::fs::write(&ae, b"pack").unwrap();
        let g = scan_one(&rom, &HashMap::new()).unwrap();
        assert!(g.has_pack);
        assert!(g.pack_path.ends_with(".ae"), "legacy: {}", g.pack_path);

        // Con los dos, gana .ay — el formato actual.
        let ay = dir.path().join("juego.ay");
        std::fs::write(&ay, b"pack").unwrap();
        let g = scan_one(&rom, &HashMap::new()).unwrap();
        assert!(g.has_pack);
        assert!(g.pack_path.ends_with(".ay"), "actual: {}", g.pack_path);
    }

    // -----------------------------------------------------------------------
    // #591 — el escaneo sin tope, incremental y cancelable
    // -----------------------------------------------------------------------

    fn rom_in(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn real_crc(p: &Path) -> String {
        format!("{:08X}", crc32fast::hash(&std::fs::read(p).unwrap()))
    }

    /// Un `.ay` mínimo que el lector del core abre, con el `[compat]` que
    /// decide de qué ROM es. `crc` vacío = un pack que no declara nada, que es
    /// el caso legacy y el que no se puede atar a ningún juego.
    fn installed_pack(dir: &Path, id: &str, crc: &str, game_id: &str) -> PathBuf {
        use std::io::Write;
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(format!("{id}.ay"));
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        z.start_file("manifest.toml", opts).unwrap();
        write!(
            z,
            "[pack]\nname = \"{id}\"\nversion = \"1.0.0\"\n\
                   game_id = \"{game_id}\"\nayther_min = \"0.1.0\"\nschema = 2\n\n\
                   [regions]\ndefault = \"NTSC\"\nsupported = [\"NTSC\"]\n"
        )
        .unwrap();
        if !crc.is_empty() {
            write!(z, "\n[compat]\nrom_crc32 = \"{crc}\"\n").unwrap();
        }
        z.finish().unwrap();
        p
    }

    // -----------------------------------------------------------------------
    // #634 — el pack instalado llega a su juego
    // -----------------------------------------------------------------------

    /// EL HUECO QUE ESTO CIERRA: la biblioteca sólo miraba al lado de la ROM, y
    /// la instalación deja el pack en otro árbol. Se podía descargar, verificar
    /// e instalar un pack y la ficha seguía diciendo «no hay pack para este
    /// juego»: todo el trabajo de distribución terminaba en algo inusable.
    #[test]
    fn an_installed_pack_finds_its_game_without_sitting_next_to_the_rom() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let packs = dir.path().join("packs");
        installed_pack(&packs, "elpack", &real_crc(&rom).to_lowercase(), "x");

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        assert!(
            !v[0].has_pack,
            "el fixture no tiene ningún .ay al lado de la ROM"
        );

        attach_installed_packs(&mut v, &index_installed_packs(&packs));

        assert!(
            v[0].has_pack,
            "el pack instalado tiene que llegar a su juego"
        );
        assert!(
            v[0].pack_path.ends_with("elpack.ay"),
            "y ser el que se usa: {}",
            v[0].pack_path
        );
        assert_eq!(v[0].available_packs.len(), 1);
    }

    /// #678 — LA CARPETA DE PACKS: lo que aparezca ahí llega a su juego SIN
    /// instalarse. Es el modelo mental de la carpeta de ROMs, y evita convertir
    /// «probar un pack» en un trámite.
    #[test]
    fn a_pack_from_the_folder_reaches_its_game_without_installing() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let collection = dir.path().join("mis-packs");
        installed_pack(&collection, "probando", &real_crc(&rom).to_lowercase(), "x");

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        let installed_packs = dir.path().join("packs"); // vacía: no se instaló nada
        attach_installed_packs(&mut v, &index_packs(&[installed_packs, collection]));

        assert!(
            v[0].has_pack,
            "el pack de la carpeta tiene que llegar sin instalarse"
        );
        assert!(
            v[0].pack_path.ends_with("probando.ay"),
            "{}",
            v[0].pack_path
        );
    }

    /// Y el INSTALADO le gana al de la carpeta: instalar es un acto explícito
    /// —«este pack quiero tenerlo puesto»— y un `.ay` que apareció en la
    /// colección no puede desplazarlo sin que nada en pantalla explique por
    /// qué cambió. El orden de `indexar_packs` es la precedencia.
    #[test]
    fn the_installed_pack_beats_the_folder_one() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let crc = real_crc(&rom).to_lowercase();
        let installed_packs = dir.path().join("packs");
        let collection = dir.path().join("mis-packs");
        installed_pack(&installed_packs, "puesto", &crc, "x");
        installed_pack(&collection, "amano", &crc, "x");

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        attach_installed_packs(&mut v, &index_packs(&[installed_packs, collection]));

        assert!(
            v[0].pack_path.ends_with("puesto.ay"),
            "manda el instalado, no el de la colección: {}",
            v[0].pack_path
        );
        assert_eq!(
            v[0].available_packs.len(),
            2,
            "pero el de la colección sigue DISPONIBLE, no desaparece"
        );
    }

    /// El vínculo sale del PACK y no del nombre del archivo. Un `.ay` que se
    /// llama como la ROM pero declara otra no se asocia — emparejar por nombre
    /// estaría mal de las dos maneras: el renombrado se pierde y el ajeno entra.
    #[test]
    fn the_link_is_the_declared_crc_and_not_the_name() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let packs = dir.path().join("packs");
        // Mismo nombre que la ROM, CRC de otra.
        installed_pack(&packs, "juego", "deadbeef", "x");
        // Nombre que no dice nada, CRC correcto.
        let matching_crc = real_crc(&rom).to_lowercase();
        installed_pack(&packs, "aaa-otro-nombre", &matching_crc, "x");

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        attach_installed_packs(&mut v, &index_installed_packs(&packs));

        assert_eq!(
            v[0].available_packs.len(),
            1,
            "sólo el que declara ESTA ROM: {:?}",
            v[0].available_packs
        );
        assert!(
            v[0].pack_path.ends_with("aaa-otro-nombre.ay"),
            "se asoció por nombre en vez de por CRC: {}",
            v[0].pack_path
        );
    }

    /// El `game_id` con la convención `crc32:…` también vale: es como declaran
    /// su ROM los packs horneados antes de que `[compat]` existiera (#294).
    /// Sin esto, todo pack viejo instalado quedaría huérfano.
    #[test]
    fn the_crc_is_also_read_from_the_legacy_game_id() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let packs = dir.path().join("packs");
        let crc = real_crc(&rom).to_lowercase();
        installed_pack(&packs, "legacy", "", &format!("crc32:{crc}"));

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        attach_installed_packs(&mut v, &index_installed_packs(&packs));
        assert!(v[0].has_pack, "un pack legacy declara su ROM en el game_id");
    }

    /// Un pack que no declara ninguna ROM no se ata a nada. Es preferible que no
    /// aparezca a que aparezca en el juego equivocado: lo segundo se ve como una
    /// remasterización rota y manda a buscar el problema donde no está.
    #[test]
    fn a_pack_that_declares_no_rom_binds_to_no_game() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let packs = dir.path().join("packs");
        installed_pack(&packs, "anonimo", "", "x");

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        attach_installed_packs(&mut v, &index_installed_packs(&packs));
        assert!(
            !v[0].has_pack,
            "sin CRC declarado no hay a qué juego atarlo"
        );
    }

    /// El pack que está AL LADO DE LA ROM sigue ganando: es el que el jugador
    /// puso a mano. Y los dos quedan en la lista, que es lo que va a permitir
    /// elegir (#633) sin volver a migrar la caché.
    #[test]
    fn the_pack_next_door_wins_and_both_stay_available() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let crc = real_crc(&rom).to_lowercase();
        // Uno al lado de la ROM y otro instalado, los dos para esta ROM.
        installed_pack(dir.path(), "juego", &crc, "x");
        let packs = dir.path().join("packs");
        installed_pack(&packs, "elinstalado", &crc, "x");

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        assert!(v[0].has_pack, "el de al lado lo encuentra el escaneo");
        attach_installed_packs(&mut v, &index_installed_packs(&packs));

        assert_eq!(v[0].available_packs.len(), 2, "los dos están disponibles");
        assert!(
            v[0].pack_path.ends_with(&"juego.ay".to_string()),
            "el de al lado de la ROM manda: {}",
            v[0].pack_path
        );
        assert_eq!(
            v[0].available_packs[0], v[0].pack_path,
            "`pack_path` es el primero de la lista, no otra elección aparte"
        );
    }

    #[test]
    fn selected_pack_survives_reload_and_rescan_and_falls_back_after_removal() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let crc = real_crc(&rom).to_lowercase();
        installed_pack(dir.path(), "juego", &crc, "x");
        let packs = dir.path().join("packs");
        installed_pack(&packs, "alternative", &crc, "x");
        let mut games = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        attach_installed_packs(&mut games, &index_installed_packs(&packs));
        games[0].pack_path = games[0].available_packs[1].clone();
        let selected = games[0].pack_path.clone();
        let cache = dir.path().join("library.json");
        Library::open(&cache).save_game(&games[0]);
        let lib = Library::open(&cache);

        let mut scanned = vec![scan_one(&rom, lib.known_games()).unwrap()];
        attach_installed_packs(&mut scanned, &index_installed_packs(&packs));
        lib.restore_pack_selection(&mut scanned);
        assert_eq!(scanned[0].pack_path, selected);
        assert_eq!(scanned[0].available_packs.len(), 2);

        std::fs::remove_file(selected).unwrap();
        let mut scanned = vec![scan_one(&rom, lib.known_games()).unwrap()];
        attach_installed_packs(&mut scanned, &index_installed_packs(&packs));
        lib.restore_pack_selection(&mut scanned);
        assert_eq!(
            scanned[0].pack_path,
            rom.with_extension("ay").to_string_lossy()
        );
    }

    /// Los archivos de trabajo del instalador no son packs. `.prev` es la
    /// generación anterior que guarda `rollback` (#614): ofrecerla sería
    /// ofrecer dos veces el mismo pack, uno de ellos viejo.
    #[test]
    fn the_index_ignores_the_installer_work_files() {
        let dir = tempfile::tempdir().unwrap();
        let packs = dir.path().join("packs");
        installed_pack(&packs, "bueno", "7b905383", "x");
        for suffix in [".ay.prev", ".ay.new", ".ay.part"] {
            std::fs::write(packs.join(format!("bueno{suffix}")), b"lo que sea").unwrap();
        }
        let idx = index_installed_packs(&packs);
        assert_eq!(idx.len(), 1, "sólo el instalado: {idx:?}");
        assert_eq!(idx[0].id, "bueno");
    }

    /// Y un directorio que no existe —nadie instaló nada todavía— no es un
    /// error: es una biblioteca sin packs instalados.
    #[test]
    fn without_a_packs_directory_nothing_happens() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"la rom");
        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        let idx = index_installed_packs(&dir.path().join("no-existe"));
        assert!(idx.is_empty());
        attach_installed_packs(&mut v, &idx);
        assert!(!v[0].has_pack);
    }

    /// EL defecto de #591. Un archivo más grande que el viejo tope de 64 MiB
    /// tiene que salir con CRC — y con el MISMO que da hashearlo de una sola
    /// vez, que es lo que garantiza que el DAT lo encuentre.
    ///
    /// El tamaño es 64 MiB + 1 a propósito: es el borde exacto donde el código
    /// viejo pasaba de devolver un CRC a devolver `""`, y el +1 obliga además a
    /// dar una vuelta final con un resto de un solo byte — el caso donde un
    /// buffer mal manejado deja datos afuera y el CRC sale plausible pero mal.
    #[test]
    fn a_rom_bigger_than_the_old_cap_gets_a_crc() {
        let dir = tempfile::tempdir().unwrap();
        let n = 64 * 1024 * 1024 + 1;
        let library_data: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
        let rom = rom_in(dir.path(), "grande.md", &library_data);

        let g = scan_one(&rom, &HashMap::new()).unwrap();
        assert!(!g.crc32.is_empty(), "el tope de 64 MiB dejaba esto vacio");
        assert_eq!(
            g.crc32,
            format!("{:08X}", crc32fast::hash(&library_data)),
            "por partes tiene que dar lo mismo que entero"
        );
        assert_eq!(g.size_bytes, n as u64);
    }

    /// El atajo incremental: misma huella (tamaño + fecha) = no se re-hashea.
    ///
    /// Se comprueba SIN cronómetro. La caché trae un CRC inventado; si el
    /// escaneo lo devuelve tal cual es porque no abrió el archivo, y si lo
    /// recalcula sale el de verdad. Medir tiempo sería una prueba floja que
    /// falla sola en una máquina cargada.
    #[test]
    fn what_did_not_change_is_not_rehashed() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"contenido");

        let real = scan_one(&rom, &HashMap::new()).unwrap();
        let mut cache = HashMap::new();
        let mut invalid_entry = real.clone();
        invalid_entry.crc32 = "DEADBEEF".to_string();
        cache.insert(real.path.clone(), invalid_entry);

        let g = scan_one(&rom, &cache).unwrap();
        assert_eq!(
            g.crc32, "DEADBEEF",
            "con la huella intacta no se abre el archivo"
        );
    }

    /// Y el límite del atajo: si el archivo cambió, se relee aunque la entrada
    /// siga en la caché. Sin esto el incremental sería una caché envenenada.
    #[test]
    fn what_changed_is_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"contenido");

        let mut cache = HashMap::new();
        let mut previous_game = scan_one(&rom, &HashMap::new()).unwrap();
        previous_game.crc32 = "DEADBEEF".to_string();
        cache.insert(previous_game.path.clone(), previous_game);

        std::fs::write(&rom, b"contenido con otro largo").unwrap();
        let g = scan_one(&rom, &cache).unwrap();
        assert_eq!(
            g.crc32,
            real_crc(&rom),
            "cambio el tamano: hay que releerlo"
        );
    }

    /// Una caché escrita antes de #591 no trae huella (`size_bytes`/`mtime` en
    /// 0 por `serde(default)`). No puede entrar al atajo: se re-hashea una vez
    /// y queda al día sola. Sin esto, toda biblioteca ya existente conservaría
    /// para siempre el CRC vacío que le dejó el tope viejo.
    #[test]
    fn the_old_cache_without_fingerprint_is_rehashed() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"contenido");

        let mut old_version = scan_one(&rom, &HashMap::new()).unwrap();
        old_version.size_bytes = 0;
        old_version.mtime = 0;
        old_version.crc32 = "DEADBEEF".to_string();
        let mut cache = HashMap::new();
        cache.insert(old_version.path.clone(), old_version);

        let g = scan_one(&rom, &cache).unwrap();
        assert_eq!(g.crc32, real_crc(&rom), "sin huella no hay atajo");
        assert_ne!(g.size_bytes, 0, "y queda con huella para la proxima");
    }

    /// Cancelar corta el escaneo, y el resultado DICE que lo cortaron — quien
    /// lo recibe tiene que poder distinguir «no hay juegos» de «no terminé».
    #[test]
    fn cancel_cuts_the_scan_short_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..8 {
            rom_in(dir.path(), &format!("j{i}.md"), b"x");
        }

        let cancel: CancelToken = Arc::new(AtomicBool::new(true));
        let out = scan_incremental(dir.path(), &HashMap::new(), &cancel, &|_, _| {});
        assert!(out.cancelled, "tiene que DECIR que lo cortaron");
        assert!(
            out.games.is_empty(),
            "cancelado de entrada no mira ningun archivo"
        );

        let cancel: CancelToken = Arc::new(AtomicBool::new(false));
        let out = scan_incremental(dir.path(), &HashMap::new(), &cancel, &|_, _| {});
        assert!(!out.cancelled);
        assert_eq!(out.games.len(), 8);
    }

    /// El progreso llega, y el total es el de archivos CANDIDATOS —no el de
    /// entradas del directorio—, que es lo que la barra necesita para no
    /// retroceder cuando la carpeta tiene cosas que no son ROMs.
    #[test]
    fn progress_reports_over_the_candidates() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..3 {
            rom_in(dir.path(), &format!("j{i}.md"), b"x");
        }
        rom_in(dir.path(), "leeme.txt", b"no soy una rom");

        let progress_updates = std::sync::Mutex::new(Vec::new());
        let cancel: CancelToken = Arc::new(AtomicBool::new(false));
        scan_incremental(dir.path(), &HashMap::new(), &cancel, &|h, t| {
            progress_updates.lock().unwrap().push((h, t))
        });

        let v = progress_updates.lock().unwrap();
        assert!(
            v.iter().all(|(_, t)| *t == 3),
            "el .txt no es candidato: {v:?}"
        );
        assert_eq!(
            v.first(),
            Some(&(0, 3)),
            "arranca en 0 para que la barra exista"
        );
        assert_eq!(v.iter().map(|(h, _)| *h).max(), Some(3), "y llega al total");
    }

    /// Mover una ROM de carpeta NO crea una entrada nueva: la vieja se re-ubica,
    /// y con ella todo lo que el escaneo no sabe re-derivar. Hoy se prueba con
    /// `name` y `region` —que los pone el DAT—; desde #593 van a ser además los
    /// favoritos y los metadatos que editó el jugador, y este mismo test los
    /// cubre sin tocarlo.
    #[test]
    fn a_moved_rom_keeps_its_entry() {
        let dir = tempfile::tempdir().unwrap();
        let before = dir.path().join("antes");
        let after = dir.path().join("despues");
        std::fs::create_dir_all(&before).unwrap();
        std::fs::create_dir_all(&after).unwrap();

        let origin = rom_in(&before, "juego.md", b"la misma rom");
        let mut lib = Library::open(&dir.path().join("library.json"));
        lib.upsert_games(&[scan_one(&origin, &HashMap::new()).unwrap()]);

        // Lo que el escaneo NO produce y tiene que sobrevivir.
        {
            let key = origin.to_string_lossy().to_string();
            let e = lib.data.games.get_mut(&key).unwrap();
            e.name = "Nombre canonico del DAT".to_string();
            e.region = "WLD".to_string();
        }

        let destination = after.join("juego.md");
        std::fs::rename(&origin, &destination).unwrap();
        lib.upsert_games(&[scan_one(&destination, &HashMap::new()).unwrap()]);

        assert_eq!(
            lib.get_all().len(),
            1,
            "una ROM movida es UNA entrada, no dos"
        );
        let e = &lib.get_all()[0];
        assert_eq!(
            e.path,
            destination.to_string_lossy(),
            "apunta a donde esta ahora"
        );
        assert_eq!(
            e.name, "Nombre canonico del DAT",
            "no se perdio lo que no re-deriva"
        );
        assert_eq!(e.region, "WLD");
    }

    // -----------------------------------------------------------------------
    // #595 — el semáforo contesta lo MISMO que el core
    // -----------------------------------------------------------------------

    /// La traducción de grados es total y ORDENADA: mejor a peor, igual que
    /// `CompatGrade`. Si el core agrega un grado, `de_grado` no compila —
    /// que es lo que se busca: un `_ =>` se lo comería y Play mostraría el
    /// grado nuevo como si fuera el peor conocido.
    #[test]
    fn each_core_grade_has_its_own_in_play() {
        use ayther_core::pack_validate::CompatGrade as G;
        assert_eq!(PackStatus::from_grade(G::Exact), PackStatus::Exact);
        assert_eq!(PackStatus::from_grade(G::Warnings), PackStatus::Warnings);
        assert_eq!(
            PackStatus::from_grade(G::Experimental),
            PackStatus::Unverified
        );
        assert_eq!(
            PackStatus::from_grade(G::Incompatible),
            PackStatus::Incompatible
        );

        // El orden del `int` que cruza a Slint es de «nada» a «peor», y el
        // badge lo usa para elegir color: invertirlo pintaría de verde un pack
        // incompatible.
        assert!(PackStatus::NoPack.as_int() < PackStatus::Exact.as_int());
        assert!(PackStatus::Exact.as_int() < PackStatus::Warnings.as_int());
        assert!(PackStatus::Warnings.as_int() < PackStatus::Unverified.as_int());
        assert!(PackStatus::Unverified.as_int() < PackStatus::Incompatible.as_int());
    }

    /// `jugable()` dice lo mismo que `CompatGrade::runnable()`. Son dos
    /// funciones porque cruzan una frontera de crate, y por eso hace falta un
    /// test que las ate: el día que una cambie sin la otra, Play bloquearía un
    /// pack que el SDK aprueba, o al revés.
    #[test]
    fn playable_says_the_same_as_the_core() {
        use ayther_core::pack_validate::CompatGrade as G;
        for g in [G::Exact, G::Warnings, G::Experimental, G::Incompatible] {
            assert_eq!(
                PackStatus::from_grade(g).is_playable(),
                g.runnable(),
                "{g:?} no coincide"
            );
        }
    }

    /// El id de plataforma que el pack declara NO es el nombre que Play muestra.
    ///
    /// Sin esta traducción `SessionCtx.platform` queda en `None` y `compat_grade`
    /// contesta `Experimental` para TODO pack — un semáforo de un solo color, que
    /// se lee como que no funciona. Es el defecto más fácil de introducir acá,
    /// porque no falla: contesta de menos.
    #[test]
    fn the_platform_translates_to_the_format_id() {
        assert_eq!(platform_id("Mega Drive"), Some("megadrive"));
        assert_eq!(platform_id("Master System"), Some("mastersystem"));
        assert_eq!(platform_id("SNES"), Some("snes"));
        // Y toda plataforma que la biblioteca sabe detectar tiene la suya: una
        // consola con extensiones reconocidas pero sin id dejaría a sus juegos
        // permanentemente sin verificar.
        for ext in [
            "md", "gen", "bin", "smc", "sfc", "nes", "gba", "n64", "z64", "gb", "gbc", "pce",
            "sms", "gg",
        ] {
            let p = platform_for_ext(ext).expect("la tabla de extensiones lo tiene");
            assert!(
                platform_id(p).is_some(),
                "la plataforma «{p}» (.{ext}) no tiene id de formato"
            );
        }
    }

    /// Sin pack no hay grado: se juega el original, y graduar la nada sería
    /// inventar un veredicto. `enrich_pack_status` tiene que dejarlo en
    /// `SinPack` aunque la entrada viniera con otra cosa de un escaneo anterior.
    #[test]
    fn without_a_pack_there_is_no_grade() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"x");
        let mut g = scan_one(&rom, &HashMap::new()).unwrap();
        g.pack_status = PackStatus::Exact; // basura de antes
        g.compat_reason = "quedó de antes".into();

        let mut v = vec![g];
        enrich_pack_status(&mut v, Some("core 1.0"));
        assert_eq!(v[0].pack_status, PackStatus::NoPack);
        assert!(
            v[0].compat_reason.is_empty(),
            "sin pack no hay nada que explicar"
        );
    }

    /// Un `.ay` que no es un pack se gradúa **Incompatible**, no «Ok».
    ///
    /// Es el defecto que #595 vino a cerrar: el criterio viejo era «¿se puede
    /// leer el manifest?», así que un archivo roto quedaba como `Corrupt` pero
    /// uno legible de OTRO juego pasaba como `Ok` — `Mismatch` no se asignaba
    /// nunca. Ahora el veredicto lo da el validador del core, el mismo que le
    /// contestó al autor.
    #[test]
    fn a_pack_that_does_not_work_grades_incompatible_and_explains_it() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"x");
        std::fs::write(dir.path().join("juego.ay"), b"esto no es un pack").unwrap();

        let mut v = vec![scan_one(&rom, &HashMap::new()).unwrap()];
        assert!(v[0].has_pack, "el fixture tiene que tener el .ay al lado");
        enrich_pack_status(&mut v, Some("core 1.0"));

        assert_eq!(v[0].pack_status, PackStatus::Incompatible);
        assert!(
            !v[0].compat_reason.is_empty(),
            "un grado sin motivo obliga a la UI a inventar el texto"
        );
    }

    /// Y el eje que #595 agrega: **sin saber el core, no se puede verificar**.
    ///
    /// El mismo pack, con y sin `core_build_id`, tiene que dar grados distintos.
    /// Sin este test, pasar `None` para siempre sería un cambio invisible: todo
    /// quedaría «no verificado» y nadie sabría que falta un dato en vez de que
    /// los packs estén mal.
    #[test]
    fn without_the_core_build_the_grade_cannot_close() {
        use ayther_core::pack_validate::{CompatGrade, SessionCtx, compat_grade};
        let dir = tempfile::tempdir().unwrap();
        let invalid_entry = dir.path().join("x.ay");
        std::fs::write(&invalid_entry, b"no es un pack").unwrap();
        let path = invalid_entry.to_string_lossy().to_string();

        // Con un pack roto manda el error, aun sin contexto: si ya se sabe que
        // no corre, decir «experimental» sería suavizar un no.
        let mut ctx = SessionCtx::default();
        assert_eq!(compat_grade(&path, &ctx).grade, CompatGrade::Incompatible);

        // Y el contexto incompleto se REPORTA, que es lo que Play muestra.
        ctx.core_build_id = None;
        assert!(
            compat_grade(&path, &ctx)
                .unverified
                .contains(&"core_build_id"),
            "el core sin sondear tiene que aparecer como no verificado"
        );
    }

    /// #593 — UN RE-ESCANEO NO BORRA LO DEL JUGADOR.
    ///
    /// Éste es el test que faltaba y el defecto que encontró. `upsert_games`
    /// reemplazaba la entrada entera cuando la ruta ya se conocía, cosa que
    /// estuvo bien mientras `Game` era puro dato del disco. Con favoritos y
    /// títulos editados adentro, apretar F5 los borraba — y no ruidosamente: el
    /// juego seguía en la lista, sólo que ya no era favorito y había vuelto a
    /// llamarse como el archivo. El escaneo produce `favorito: false` y
    /// `titulo_usuario: None` porque el disco no sabe otra cosa; conservarlo es
    /// trabajo de la fusión, no del escaneo.
    #[test]
    fn a_rescan_does_not_erase_favorites_or_edits() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"contenido");
        let mut lib = Library::open(&dir.path().join("library.json"));

        // Primer escaneo, y el jugador toca sus cosas.
        lib.upsert_games(&[scan_one(&rom, &HashMap::new()).unwrap()]);
        let mut customized_game = lib.get_all()[0].clone();
        customized_game.favorite = true;
        customized_game.user_title = Some("Como yo le digo".into());
        customized_game.user_art = Some("covers/custom.png".into());
        customized_game.last_played = 12345;
        customized_game.seconds_played = 600;
        lib.save_game(&customized_game);

        // F5: el escaneo vuelve a producir la MISMA ruta, con los campos del
        // jugador en su valor por defecto.
        let rescanned_game = scan_one(&rom, &HashMap::new()).unwrap();
        assert!(
            !rescanned_game.favorite && rescanned_game.user_title.is_none(),
            "el fixture tiene que representar lo que el escaneo produce"
        );
        lib.upsert_games(&[rescanned_game]);

        let g = &lib.get_all()[0];
        assert!(g.favorite, "el favorito no sobrevivio al re-escaneo");
        assert_eq!(g.title(), "Como yo le digo", "el titulo editado se perdio");
        assert_eq!(g.artwork(), "covers/custom.png");
        let reopened = Library::open(&dir.path().join("library.json"));
        assert_eq!(reopened.get_all()[0].artwork(), "covers/custom.png");
        assert_eq!(g.last_played, 12345);
        assert_eq!(g.seconds_played, 600);
        // Y lo del disco SÍ se actualiza: para eso corre el escaneo.
        assert_eq!(g.name, "juego", "lo automatico tiene que venir del escaneo");
    }

    /// #594 — EL CA: el tiempo jugado se persiste POR JUEGO y se ACUMULA.
    ///
    /// Hasta ahora `played_secs` moría en la encuesta (`spawn.rs`): se usaba
    /// para decidir si preguntar y se tiraba. Sin acumular, «12 h 40 m» sería el
    /// tiempo de la última partida y no el del juego, que es lo que hace
    /// reconocible una biblioteca.
    #[test]
    fn play_time_accumulates_per_game() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"x");
        let mut lib = Library::open(&dir.path().join("library.json"));
        lib.upsert_games(&[scan_one(&rom, &HashMap::new()).unwrap()]);
        let path = rom.to_string_lossy().to_string();

        assert!(lib.record_session(&path, 600, 1000));
        assert!(lib.record_session(&path, 300, 2000));

        let g = &lib.get_all()[0];
        assert_eq!(g.seconds_played, 900, "las dos sesiones se suman");
        assert_eq!(g.last_played, 2000, "y la fecha es la de la ULTIMA");

        // Y sobrevive al re-escaneo, como el resto de lo del jugador.
        lib.upsert_games(&[scan_one(&rom, &HashMap::new()).unwrap()]);
        assert_eq!(lib.get_all()[0].seconds_played, 900);
    }

    /// Lanzar algo que ya no está en la biblioteca no es un error: no hay dónde
    /// anotarlo, y decirlo es mejor que crear una entrada fantasma.
    #[test]
    fn recording_an_unknown_rom_does_not_invent_an_entry() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(&dir.path().join("library.json"));
        assert!(!lib.record_session("D:/no/existe.md", 60, 1));
        assert!(lib.get_all().is_empty());
    }

    /// El otro lado: `guardar` reemplaza, porque viene de que el jugador acaba
    /// de cambiarlo. Si fusionara como el escaneo, marcar favorito sería un
    /// no-op — se leería el valor viejo encima del nuevo.
    #[test]
    fn saving_overwrites_what_the_player_had_on_purpose() {
        let dir = tempfile::tempdir().unwrap();
        let rom = rom_in(dir.path(), "juego.md", b"x");
        let mut lib = Library::open(&dir.path().join("library.json"));
        lib.upsert_games(&[scan_one(&rom, &HashMap::new()).unwrap()]);

        let mut g = lib.get_all()[0].clone();
        g.favorite = true;
        lib.save_game(&g);
        assert!(lib.get_all()[0].favorite);

        g.favorite = false;
        lib.save_game(&g);
        assert!(
            !lib.get_all()[0].favorite,
            "desmarcar tiene que poder deshacerse"
        );
    }

    /// Y el límite de eso: dos COPIAS del mismo juego, las dos existiendo, son
    /// dos entradas. Tomarlas por una mudanza haría desaparecer la mitad de una
    /// biblioteca con duplicados legítimos.
    #[test]
    fn two_live_copies_of_the_same_game_are_two_entries() {
        let dir = tempfile::tempdir().unwrap();
        let a = rom_in(dir.path(), "copia_a.md", b"identicas");
        let b = rom_in(dir.path(), "copia_b.md", b"identicas");

        let ga = scan_one(&a, &HashMap::new()).unwrap();
        let gb = scan_one(&b, &HashMap::new()).unwrap();
        assert_eq!(
            ga.crc32, gb.crc32,
            "el fixture tiene que tener el mismo CRC"
        );

        let mut lib = Library::open(&dir.path().join("library.json"));
        lib.upsert_games(&[ga]);
        lib.upsert_games(&[gb]);
        assert_eq!(lib.get_all().len(), 2, "las dos existen: son dos entradas");
    }

    /// La caché se escribe entera o no se escribe: nunca un JSON a medias.
    /// Un JSON truncado no parsea, y `Library::open` cae a `unwrap_or_default()`
    /// — o sea que el jugador arranca con la biblioteca VACÍA y sin un aviso.
    #[test]
    fn saving_is_atomic_and_leaves_no_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let rom = rom_in(dir.path(), "juego.md", b"x");

        let mut lib = Library::open(&path);
        lib.upsert_games(&[scan_one(&rom, &HashMap::new()).unwrap()]);

        assert!(path.exists(), "el destino quedo escrito");
        assert!(
            !path.with_extension("json.tmp").exists(),
            "el temporal no sobrevive al commit"
        );
        assert_eq!(
            Library::open(&path).get_all().len(),
            1,
            "y lo escrito se relee"
        );
    }
}
