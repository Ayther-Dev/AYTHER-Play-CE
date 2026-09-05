//! spawn.rs — launch `ayther_runtime` for a game session.
//!
//! Extracted from the M2 thin slice. The key contract:
//!   - Spawns the runtime as a child process (non-blocking for the caller).
//!   - Drains the child's stdout on a background thread, classifying
//!     `AYTHER_STATUS <json>` lifecycle events from plain runtime logs.
//!   - Calls `on_exit` (on the background thread) with whether the session
//!     ended cleanly or crashed — the caller can invoke_from_event_loop to
//!     update the Slint UI.
//!   - Never blocks the Slint event-loop thread.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

/// Result returned (on a background thread) when a game session ends.
#[derive(Debug, Clone)]
pub struct SessionResult {
    pub game_name: String,
    pub clean_exit: bool,
    pub exit_code: Option<i32>,
    /// Stable startup failure reported by Runtime, if any.
    pub failure_reason: String,
    /// Path to the serialized savestate emitted by the runtime on clean exit
    /// (`AYTHER_STATUS {"event":"exit","savestate":"path"}`).
    /// Empty string if no savestate was written (crashed or disabled).
    pub savestate_path: String,
    /// #313: cuánto duró la sesión. Es lo que decide si tiene sentido pedir
    /// una opinión: por dos minutos de juego no hay opinión formada, y
    /// preguntar igual enseña al jugador a ignorar el formulario.
    pub played_secs: u64,
}

/// Spawn `ayther_runtime` for a game and call `on_exit` when it finishes.
///
/// Returns immediately — the runtime runs in a background OS process while the
/// lifecycle is drained on a spawned Rust thread.
///
/// # Arguments
/// - `runtime`   — path to `ayther_runtime` exe
/// - `core`      — path to the Libretro core DLL (BYOC)
/// - `rom`       — path to the ROM file (BYOR)
/// - `pack`      — optional path to the HD pack `.ay`
/// - `game_name` — display name for status messages
/// - `on_exit`   — called from the background thread with the session result
pub struct LaunchRequest<'a> {
    pub runtime: &'a Path,
    pub core: &'a Path,
    pub rom: &'a Path,
    pub pack: Option<&'a Path>,
    pub game_name: String,
    // #311: lo que el jugador eligio en la pantalla previa. `None` = no
    // se configuro nada y manda el pack, que es lo que hacia antes de
    // que esta pantalla existiera.
    pub cfg: Option<&'a crate::launch_config::LaunchConfig>,
    // #596 — la ruta del `launch.toml` de ESTA sesión, ya escrito.
    //
    // Va además de los argumentos y no en su lugar: el runtime de hoy no lee
    // manifests, y cambiar las dos cosas a la vez dejaría a Play sin poder
    // lanzar hasta que el runtime se actualice. Con `--manifest` presente, un
    // runtime que lo entienda tiene el archivo entero; uno que no, lo ignora
    // como cualquier argumento desconocido y usa los mismos flags de siempre —
    // que salen del mismo manifest, así que dicen lo mismo.
    pub manifest: Option<&'a Path>,
    // #603 - donde viven los guardados de ESTE jugador, y con que ROM se juega.
    //
    // La raiz la manda Play y no la deduce el Runtime: eran dos carpetas
    // distintas —el Runtime escribia en la suya— asi que Play no encontraba los
    // guardados que el juego generaba. Un guardado que el jugador tiene y su
    // launcher no ve es un guardado perdido.
    pub saves_dir: &'a Path,
    pub rom_crc32: &'a str,
    // Los controles del jugador (Ajustes › Controles del juego), ya escritos
    // en su TOML. `None` = nunca cambió nada, y el runtime usa su fábrica —
    // la flag sólo viaja cuando dice algo.
    pub input_map: Option<&'a Path>,
    // #604 - desde qué guardado reanudar. `None` = partida limpia.
    //
    // Es una RUTA y no una bandera «reanudá si podés»: quien elige cuál de las
    // versiones se carga es Play, que es el único que las tiene listadas y el
    // único que le preguntó al jugador.
    pub save: Option<&'a Path>,
}

pub fn launch<E, F>(
    request: LaunchRequest<'_>,
    // #600 - cada evento del protocolo, tipado, a medida que llega.
    //
    // Va aparte de `on_exit` porque responde otra pregunta: `on_exit` dice como
    // termino, y esto dice que esta pasando MIENTRAS. Sin el, entre apretar A y
    // ver el juego no hay nada en pantalla, y esos segundos se leen como que no
    // paso nada - que es como se lanzan dos sesiones del mismo juego.
    on_event: E,
    on_exit: F,
) where
    E: Fn(&crate::runtime_event::RuntimeEvent) + Send + 'static,
    F: FnOnce(SessionResult) + Send + 'static,
{
    let LaunchRequest {
        runtime,
        core,
        rom,
        pack,
        game_name,
        cfg,
        manifest,
        saves_dir,
        rom_crc32,
        input_map,
        save,
    } = request;
    let mut cmd = Command::new(runtime);
    cmd.args(["--core", &core.to_string_lossy()]);
    cmd.args(["--rom", &rom.to_string_lossy()]);
    if let Some(p) = pack
        && !p.as_os_str().is_empty()
    {
        cmd.args(["--pack", &p.to_string_lossy()]);
    }
    // #311: la configuracion previa viaja como argumentos y no como un
    // archivo que el runtime tenga que encontrar: asi lo que se lanzo
    // queda escrito en la linea de comandos, y una sesion se puede
    // reproducir copiandola.
    if let Some(c) = cfg {
        for a in c.to_args() {
            cmd.arg(a);
        }
    }
    // #596: y la sesión entera, en un archivo. Es lo que hace reproducible una
    // partida —copiar el `launch.toml` y volver a lanzarlo— y la base del modo
    // seguro (#601), que necesita saber qué había para poder quitarlo.
    if let Some(m) = manifest {
        cmd.args(["--manifest", &m.to_string_lossy()]);
    }
    // #603: el layout de guardados es <saves>/<game_id>/<perfil>/, y la
    // revision de la ROM viaja en el nombre de cada version — asi sobrevive a
    // copiar la carpeta, que es lo que la gente hace con sus guardados.
    cmd.args(["--saves-dir", &saves_dir.to_string_lossy()]);
    if let Some(im) = input_map {
        cmd.args(["--input-map", &im.to_string_lossy()]);
    }
    if let Some(g) = save {
        cmd.args(["--load-state", &g.to_string_lossy()]);
    }
    if !rom_crc32.is_empty() {
        cmd.args(["--rom-crc32", rom_crc32]);
    }
    cmd.stdout(Stdio::piped());
    // #600: el stderr TAMBIÉN. Antes iba a la consola del padre —o a la nada,
    // en una build de ventana— así que un core que no cargaba dejaba a Play
    // diciendo «terminó de forma anormal» sin una sola línea de por qué. Es el
    // caso donde el log importa más y era el único que no se guardaba.
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            crate::log_error!("spawn", "no se pudo lanzar {}: {e}", runtime.display());
            on_exit(SessionResult {
                game_name,
                clean_exit: false,
                exit_code: None,
                failure_reason: String::new(),
                savestate_path: String::new(),
                played_secs: 0,
            });
            return;
        }
    };

    eprintln!("[play] Spawned ayther_runtime (pid {})", child.id());
    // #313: el reloj arranca acá, con el proceso ya vivo — no antes, para no
    // contarle al jugador el tiempo que tardó en abrir.
    let started = std::time::Instant::now();

    // Drain stdout on a background thread so the child pipe never fills.
    std::thread::spawn(move || {
        let mut savestate_path = String::new();
        let mut failure_reason = String::new();

        // #600: el stderr se drena en SU hilo. Con uno solo, un stderr que se
        // llena mientras nadie lo lee bloquea al hijo — y el síntoma sería un
        // juego que se cuelga al arrancar, que es lo último que uno atribuiría
        // a un pipe.
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for l in BufReader::new(stderr).lines().map_while(Result::ok) {
                    eprintln!("[runtime!] {l}");
                }
            });
        }

        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                // #600: el protocolo se DESERIALIZA. Antes se buscaban
                // subcadenas —`contains("\"exit\"")` y un escaneo a mano del
                // campo `savestate`— y eso confundía eventos con cualquier línea
                // que llevara la palabra, y partía una ruta con comillas por la
                // mitad. Ninguna de las dos cosas daba error: la sesión
                // simplemente no guardaba.
                match crate::runtime_event::classify(&line) {
                    crate::runtime_event::ParsedLine::Event(e) => {
                        match &e {
                            crate::runtime_event::RuntimeEvent::Warning { reason }
                                if reason == "input.map_invalid" =>
                            {
                                failure_reason = reason.clone()
                            }
                            crate::runtime_event::RuntimeEvent::Error { reason } => {
                                failure_reason = reason.clone()
                            }
                            _ => {}
                        }
                        if let crate::runtime_event::RuntimeEvent::Exit { savestate } = &e {
                            savestate_path = savestate.clone();
                        }
                        record(&e);
                        on_event(&e);
                    }
                    crate::runtime_event::ParsedLine::Malformed(t) => {
                        // Se REPORTA en vez de tirarse: una línea del protocolo
                        // que no parsea es un síntoma, y el silencio esconde
                        // justo el caso que hay que investigar.
                        eprintln!("[play] AYTHER_STATUS que no se entiende: {t}");
                    }
                    crate::runtime_event::ParsedLine::Log(l) => eprintln!("[runtime] {l}"),
                }
            }
        }

        // Reap the child.
        let result = match child.wait() {
            Ok(status) => {
                let clean = status.success() && failure_reason.is_empty();
                let code = status.code();
                if clean {
                    eprintln!("[play] Session ended cleanly.");
                } else {
                    eprintln!(
                        "[play] Session ended ABNORMALLY (exit {:?}) — crash firewall held.",
                        code
                    );
                }
                SessionResult {
                    game_name,
                    clean_exit: clean,
                    exit_code: code,
                    failure_reason,
                    savestate_path,
                    played_secs: started.elapsed().as_secs(),
                }
            }
            Err(e) => {
                eprintln!("[play] Could not reap runtime: {e}");
                SessionResult {
                    game_name,
                    clean_exit: false,
                    exit_code: None,
                    failure_reason,
                    savestate_path: String::new(),
                    played_secs: started.elapsed().as_secs(),
                }
            }
        };

        on_exit(result);
    });
}

/// #600: la línea de log de cada evento, elegida por TIPO.
///
/// La versión anterior volvía a buscar subcadenas para decidir la etiqueta —el
/// mismo `contains` que el parser, repetido— así que un evento nuevo salía
/// impreso como JSON crudo y uno cuyo texto mencionara «exit» se anunciaba como
/// una salida limpia. Con el enum, agregar un evento no compila hasta que
/// alguien decida cómo se anuncia.
fn record(e: &crate::runtime_event::RuntimeEvent) {
    use crate::runtime_event::RuntimeEvent as E;
    match e {
        E::Ready {
            game_id, has_pack, ..
        } => eprintln!(
            "[play] sesión lista — {game_id} (pack: {})",
            if *has_pack != 0 { "sí" } else { "no" }
        ),
        E::NowPlaying { title, game_id } => eprintln!(
            "[play] jugando: {}",
            if title.is_empty() { game_id } else { title }
        ),
        // Un warning NO corta la sesión: tratarlo como error cerraría una
        // partida que estaba andando.
        E::Warning { reason } => eprintln!("[play] aviso del runtime: {reason}"),
        E::Error { reason } => eprintln!("[play] ERROR del runtime: {reason}"),
        E::Exit { savestate } => eprintln!(
            "[play] sesión terminada{}",
            if savestate.is_empty() {
                String::new()
            } else {
                format!(" — guardado en {savestate}")
            }
        ),
        E::CrashTest => eprintln!("[play] crash-test: el hijo va a abortar a propósito"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "integration: requires the pinned distributed AYTHER_RUNTIME_EXE"]
    fn invalid_input_maps_fail_through_play_launch() {
        let runtime = std::path::PathBuf::from(
            std::env::var_os("AYTHER_RUNTIME_EXE").expect("AYTHER_RUNTIME_EXE is required"),
        );
        assert!(runtime.is_file(), "Runtime distribution is required");
        let temp = tempfile::tempdir().unwrap();
        for content in [
            None,
            Some("[keyboard"),
            Some("[keyboard]\na = \"not an SDL key\"\n"),
            Some("[gamepad]\nup = \"south\"\n"),
        ] {
            let map = temp.path().join("input map with spaces.toml");
            if let Some(content) = content {
                std::fs::write(&map, content).unwrap();
            }
            let (send_result, result) = std::sync::mpsc::channel();
            let (send_event, events) = std::sync::mpsc::channel();
            launch(
                LaunchRequest {
                    runtime: &runtime,
                    core: Path::new("unused-core"),
                    rom: Path::new("unused-rom"),
                    pack: None,
                    game_name: "input map regression".into(),
                    cfg: None,
                    manifest: None,
                    saves_dir: temp.path(),
                    rom_crc32: "",
                    input_map: Some(&map),
                    save: None,
                },
                move |event| {
                    send_event.send(event.clone()).unwrap();
                },
                move |value| {
                    send_result.send(value).unwrap();
                },
            );
            let result = result
                .recv_timeout(std::time::Duration::from_secs(20))
                .expect("Runtime must fail promptly");
            assert!(!result.clean_exit);
            assert_eq!(result.exit_code, Some(78));
            assert_eq!(result.failure_reason, "input.map_invalid");
            assert!(result.savestate_path.is_empty());
            assert!(!events.try_iter().any(|event| matches!(
                event,
                crate::runtime_event::RuntimeEvent::Ready { .. }
                    | crate::runtime_event::RuntimeEvent::NowPlaying { .. }
            )));
        }
    }
}
