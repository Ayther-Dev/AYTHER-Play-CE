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
    pub failure: crate::launch_failure::Failure,
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
    pub reservations: &'a [std::sync::Arc<std::fs::File>],
    pub runtime: &'a Path,
    pub core: &'a Path,
    pub rom: &'a Path,
    pub pack: Option<&'a Path>,
    pub trust_registry: Option<&'a Path>,
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

fn append_ordinary_runtime_arguments(cmd: &mut Command, request: &LaunchRequest<'_>) {
    cmd.args(["--core", &request.core.to_string_lossy()]);
    cmd.args(["--rom", &request.rom.to_string_lossy()]);
    if let Some(pack) = request.pack
        && !pack.as_os_str().is_empty()
    {
        cmd.args(["--pack", &pack.to_string_lossy()]);
        if let Some(registry) = request.trust_registry {
            cmd.arg("--trust-registry").arg(registry);
        }
    }
    // The selected launch configuration remains explicit in the command so a
    // normal session can be reproduced from the same effective conditions.
    if let Some(config) = request.cfg {
        cmd.args(config.to_args());
    }
    if let Some(manifest) = request.manifest {
        cmd.args(["--manifest", &manifest.to_string_lossy()]);
    }
    cmd.args(["--saves-dir", &request.saves_dir.to_string_lossy()]);
    if let Some(input_map) = request.input_map {
        cmd.args(["--input-map", &input_map.to_string_lossy()]);
    }
    if let Some(save) = request.save {
        cmd.args(["--load-state", &save.to_string_lossy()]);
    }
    if !request.rom_crc32.is_empty() {
        cmd.args(["--rom-crc32", request.rom_crc32]);
    }
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
    let reservations = request.reservations.to_vec();
    let require_reservations = !reservations.is_empty();
    let runtime_path = request.runtime;
    let mut cmd = Command::new(runtime_path);
    for (file, flag) in reservations
        .iter()
        .zip(["--rom-revision", "--pack-revision"])
    {
        match crate::file_revision::Revision::from_file(file) {
            Ok(revision) => {
                cmd.arg(flag).arg(revision.runtime_token());
            }
            Err(error) => {
                on_exit(SessionResult {
                    game_name: request.game_name,
                    clean_exit: false,
                    exit_code: None,
                    failure: crate::launch_failure::Failure {
                        reason: "runtime.reservation_failed".into(),
                        detail: error.to_string(),
                        ..Default::default()
                    },
                    savestate_path: String::new(),
                    played_secs: 0,
                });
                return;
            }
        }
    }
    append_ordinary_runtime_arguments(&mut cmd, &request);
    let game_name = request.game_name;
    cmd.stdout(Stdio::piped());
    // #600: el stderr TAMBIÉN. Antes iba a la consola del padre —o a la nada,
    // en una build de ventana— así que un core que no cargaba dejaba a Play
    // diciendo «terminó de forma anormal» sin una sola línea de por qué. Es el
    // caso donde el log importa más y era el único que no se guardaba.
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            crate::log_error!("spawn", "no se pudo lanzar {}: {e}", runtime_path.display());
            on_exit(SessionResult {
                game_name,
                clean_exit: false,
                exit_code: None,
                failure: crate::launch_failure::Failure {
                    reason: "runtime.spawn_failed".into(),
                    detail: crate::logging::redact(&e.to_string()),
                    ..Default::default()
                },
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
        let _reservations = reservations;
        let mut reservations_confirmed = !require_reservations;
        let mut savestate_path = String::new();
        let mut failure = crate::launch_failure::Failure::default();

        // #600: el stderr se drena en SU hilo. Con uno solo, un stderr que se
        // llena mientras nadie lo lee bloquea al hijo — y el síntoma sería un
        // juego que se cuelga al arrancar, que es lo último que uno atribuiría
        // a un pipe.
        let stderr_reader = child.stderr.take().map(|stderr| {
            std::thread::spawn(move || {
                let mut diagnostic = String::new();
                for l in BufReader::new(stderr).lines().map_while(Result::ok) {
                    eprintln!("[runtime!] {l}");
                    if diagnostic.len() < 8192 {
                        diagnostic.extend(l.chars().take(8192 - diagnostic.len()));
                        diagnostic.push('\n');
                    }
                }
                diagnostic
            })
        });

        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.trim() == "AYTHER_RESERVATION 1" {
                    reservations_confirmed = true;
                    continue;
                }
                // #600: el protocolo se DESERIALIZA. Antes se buscaban
                // subcadenas —`contains("\"exit\"")` y un escaneo a mano del
                // campo `savestate`— y eso confundía eventos con cualquier línea
                // que llevara la palabra, y partía una ruta con comillas por la
                // mitad. Ninguna de las dos cosas daba error: la sesión
                // simplemente no guardaba.
                match crate::runtime_event::classify(&line) {
                    crate::runtime_event::ParsedLine::Event(e) => {
                        if matches!(e, crate::runtime_event::RuntimeEvent::Ready { .. })
                            && !reservations_confirmed
                        {
                            failure.reason = "runtime.reservation_unsupported".into();
                            failure.explicit_error = true;
                            failure.detail = "Este Runtime no confirmó los archivos preparados. Actualizá Runtime junto con Play CE.".into();
                            let _ = child.kill();
                            break;
                        }
                        failure.observe(&e);
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
        let exit = child.wait();
        let stderr = stderr_reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();
        failure.finish(exit.as_ref().ok().and_then(|status| status.code()), &stderr);
        if !reservations_confirmed {
            failure.reservation_not_confirmed();
        }
        let result = match exit {
            Ok(status) => {
                let clean = status.success() && !failure.explicit_error;
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
                    failure,
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
                    failure,
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
        E::Warning { reason, .. } => eprintln!("[play] aviso del runtime: {reason}"),
        E::Error { reason, .. } => eprintln!("[play] ERROR del runtime: {reason}"),
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
    fn ordinary_launch_preserves_the_existing_runtime_arguments() {
        let request = LaunchRequest {
            reservations: &[],
            runtime: Path::new("runtime"),
            core: Path::new("core.dll"),
            rom: Path::new("game.rom"),
            pack: Some(Path::new("pack.ay")),
            trust_registry: Some(Path::new("trust.toml")),
            game_name: "ordinary".into(),
            cfg: None,
            manifest: Some(Path::new("launch.toml")),
            saves_dir: Path::new("saves"),
            rom_crc32: "1234abcd",
            input_map: Some(Path::new("input.toml")),
            save: Some(Path::new("slot.state")),
        };
        let mut command = Command::new(request.runtime);
        append_ordinary_runtime_arguments(&mut command, &request);
        let arguments: Vec<_> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            arguments,
            [
                "--core",
                "core.dll",
                "--rom",
                "game.rom",
                "--pack",
                "pack.ay",
                "--trust-registry",
                "trust.toml",
                "--manifest",
                "launch.toml",
                "--saves-dir",
                "saves",
                "--input-map",
                "input.toml",
                "--load-state",
                "slot.state",
                "--rom-crc32",
                "1234abcd",
            ]
        );
    }

    #[test]
    #[ignore = "integration: requires AYTHER_RUNTIME_EXE built with reservations"]
    fn runtime_accepts_play_revision_and_rejects_another_revision() {
        let runtime = std::path::PathBuf::from(
            std::env::var_os("AYTHER_RUNTIME_EXE").expect("AYTHER_RUNTIME_EXE"),
        );
        let d = tempfile::tempdir().unwrap();
        let rom = d.path().join("rom.bin");
        let pack = d.path().join("pack.ay");
        std::fs::write(&rom, b"rom fixture").unwrap();
        std::fs::write(&pack, b"pack fixture").unwrap();
        let rom_guard = crate::file_revision::reserve(&rom).unwrap();
        let pack_guard = crate::file_revision::reserve(&pack).unwrap();
        let rom_revision = crate::file_revision::Revision::from_file(&rom_guard)
            .unwrap()
            .runtime_token();
        let pack_revision = crate::file_revision::Revision::from_file(&pack_guard)
            .unwrap()
            .runtime_token();
        let run = |revision: &str| {
            std::process::Command::new(&runtime)
                .arg("--core")
                .arg(d.path().join("unused-core.dll"))
                .arg("--rom")
                .arg(&rom)
                .arg("--pack")
                .arg(&pack)
                .arg("--rom-revision")
                .arg(revision)
                .arg("--pack-revision")
                .arg(&pack_revision)
                // Stop after the reservation handshake, before opening SDL or a core.
                .arg("--trust-registry")
                .arg(d.path().join("missing-registry.toml"))
                .output()
                .unwrap()
        };
        let accepted = run(&rom_revision);
        assert!(
            String::from_utf8_lossy(&accepted.stdout).contains("AYTHER_RESERVATION 1"),
            "{}",
            String::from_utf8_lossy(&accepted.stderr)
        );
        let rejected = run("wrong-revision");
        assert!(!rejected.status.success());
        assert!(!String::from_utf8_lossy(&rejected.stdout).contains("AYTHER_RESERVATION 1"));
    }

    #[test]
    #[ignore = "integration: requires the pinned distributed AYTHER_RUNTIME_EXE"]
    fn registry_is_forwarded_only_with_a_pack_and_startup_rejections_are_preserved() {
        let runtime = std::path::PathBuf::from(
            std::env::var_os("AYTHER_RUNTIME_EXE").expect("AYTHER_RUNTIME_EXE is required"),
        );
        let temp = tempfile::tempdir().unwrap();
        let missing_registry = temp.path().join("missing registry.toml");
        let missing_map = temp.path().join("missing map.toml");
        for (pack, registry, expected) in [
            (
                Some(Path::new("missing.ay")),
                Some(missing_registry.as_path()),
                "pack.trust_registry_invalid",
            ),
            (Some(Path::new("missing.ay")), None, "pack.open_failed"),
            (None, Some(missing_registry.as_path()), "input.map_invalid"),
        ] {
            let (sender, receiver) = std::sync::mpsc::channel();
            launch(
                LaunchRequest {
                    reservations: &[],
                    runtime: &runtime,
                    core: Path::new("unused-core"),
                    rom: Path::new("unused-rom"),
                    pack,
                    trust_registry: registry,
                    game_name: "startup regression".into(),
                    cfg: None,
                    manifest: None,
                    saves_dir: temp.path(),
                    rom_crc32: "",
                    input_map: Some(&missing_map),
                    save: None,
                },
                |_| {},
                move |result| {
                    sender.send(result).unwrap();
                },
            );
            let result = receiver
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap();
            assert!(!result.clean_exit);
            assert!(!result.failure.started);
            assert_eq!(result.failure.reason, expected);
            assert!(!result.failure.detail.is_empty());
            assert!(
                !result
                    .failure
                    .detail
                    .contains(&temp.path().to_string_lossy().to_string())
            );
        }
    }

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
                    reservations: &[],
                    runtime: &runtime,
                    core: Path::new("unused-core"),
                    rom: Path::new("unused-rom"),
                    pack: None,
                    trust_registry: None,
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
            assert_eq!(result.failure.reason, "input.map_invalid");
            assert!(result.savestate_path.is_empty());
            assert!(!events.try_iter().any(|event| matches!(
                event,
                crate::runtime_event::RuntimeEvent::Ready { .. }
                    | crate::runtime_event::RuntimeEvent::NowPlaying { .. }
            )));
        }
    }
}
