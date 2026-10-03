//! # AYTHER Play CE — el launcher abierto (ADR-005)
//!
//! Biblioteca, compatibilidad, lanzamiento, guardados locales e interfaz
//! 10-foot. **Sin ningún servicio del otro lado**: no hay catálogo que
//! consultar, ni descarga, ni cuenta, ni auto-actualización. Los packs entran
//! desde el disco y se auditan acá mismo (`verifier.rs`).
//!
//! ## Arquitectura
//!
//! ```text
//! Slint event loop (hilo principal)
//!   ├── on_launch()       → spawn::launch()          (hilo de fondo)
//!   ├── on_rescan()       → escaneo de la biblioteca (hilo de fondo)
//!   └── on_abrir_ficha()  → pack_details::read()     (hilo de fondo)
//!                           └── invoke_from_event_loop() → actualiza el modelo
//! ```
//!
//! Lo único que sale a un hilo es lo que puede tardar: escanear la carpeta de
//! ROMs y auditar un `.ay` de cientos de megas. Nada de eso toca la red, así
//! que la ventana no depende de que haya conexión para abrir.

slint::include_modules!();

mod artwork;
mod config;
// #622: el paquete de diagnóstico. La pantalla que lo muestra entra con su nota
// de diseño; el paquete y su redacción los sostienen sus oráculos.
#[cfg_attr(not(test), allow(dead_code))]
mod core_probe; // #589: validar el core del jugador sin cargarlo en Play
mod cores; // #642: qué corre cada core, según el sondeo y no según el archivo
mod date;
#[cfg_attr(not(test), allow(dead_code))]
mod diagnostics;
mod explorer; // #588: elegir carpeta y core con el mando, sin diálogo del sistema
mod game_details; // #597: los tres ejes de compatibilidad de la ficha        // #590: la hora UTC, escrita como la lee una persona
mod onboarding; // #588: el primer arranque, en seis pasos
#[macro_use]
mod i18n; // #585: la UI habla el idioma del jugador (msgid en español)
mod feedback;
mod gamepad;
mod i18n_catalog; // GENERADO por play-ce/tools/gen_translations.py
mod launch_config;
mod launch_failure;
mod launch_manifest;
// #616: cuándo se actualiza un pack instalado. La decisión es pura y vive
// acá; quién la ejecuta y la fila de Ajustes que elige la política entran con
// su nota de diseño. Hasta entonces la sostienen sus oráculos.
mod cache_metrics;
mod catalog;
mod catalog_scan;
mod file_revision;
mod inventory;
mod io_budget;
mod library;
mod library_view;
mod nointro;
#[cfg_attr(not(test), allow(dead_code))]
mod settings; // #599: de dónde sale cada valor
// #620: llevarse los datos y traerlos de vuelta. Las pantallas entran con su
// nota de diseño; el formato y la regla de «importar agrega» los sostienen sus
// oráculos.
#[cfg_attr(not(test), allow(dead_code))]
mod porting;
// #623: la telemetría, APAGADA por defecto. El módulo existe para que agrandar
// lo que se manda tenga que ser una decisión explícita y no un `push` más.
mod controls; // los controles del JUEGO: qué tecla/botón hace cada acción
mod install; // #678: un pack entra desde el disco, con el mando
mod logging; // #621: el log, con niveles y con redacción
mod pack_cache;
mod pack_details; // #312: la ficha del pack — lo declarado y lo auditado
mod pack_install;
mod pack_meta;
mod pad_router; // #586: el ruteo del mando, en un solo lugar y testeable
mod runtime_event; // #600: el protocolo AYTHER_STATUS, tipado
#[cfg(test)]
mod runtime_source; // una sola regla para los contratos contra AYTHER-Runtime
mod saves; // #603: SRAM y savestates por juego, perfil y revisión de ROM
mod session;
mod spawn;
mod verifier;

use slint::{Model, ModelRc, SharedString, VecModel};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(test)]
#[test]
#[ignore = "reads the local library; run explicitly to measure startup work"]
fn profile_local_startup() {
    let cfg = config::Config::load();
    let started = std::time::Instant::now();
    let lib = library::Library::open(&cfg.lib_path);
    let mut games = lib.get_all();
    eprintln!("cache: {:?}, {} games", started.elapsed(), games.len());
    let started = std::time::Instant::now();
    library::find_covers(&mut games, &cfg.covers_dir);
    eprintln!("covers: {:?}", started.elapsed());
    let started = std::time::Instant::now();
    for game in &games {
        let artwork = game.artwork();
        if !artwork.is_empty() {
            let started = std::time::Instant::now();
            let _image = slint::Image::load_from_path(std::path::Path::new(&artwork));
            eprintln!("cover decode: {:?}", started.elapsed());
        }
    }
    eprintln!("all cover images: {:?}", started.elapsed());
    let thumbnails = tempfile::tempdir().unwrap();
    for phase in ["cold thumbnails", "cached thumbnails"] {
        let started = std::time::Instant::now();
        for game in &games {
            let artwork = game.artwork();
            if !artwork.is_empty() {
                let _ = artwork::decode(std::path::Path::new(&artwork), thumbnails.path());
            }
        }
        eprintln!("{phase}: {:?}", started.elapsed());
    }
    let started = std::time::Instant::now();
    let dirs: Vec<_> = std::iter::once(cfg.packs_install_dir)
        .chain(cfg.pack_dirs)
        .collect();
    let packs = library::index_packs(&dirs);
    eprintln!("pack index: {:?}, {} packs", started.elapsed(), packs.len());
    library::attach_installed_packs(&mut games, &packs);
    let started = std::time::Instant::now();
    library::enrich_pack_status(&mut games, None);
    eprintln!("pack compatibility: {:?}", started.elapsed());
}

fn main() {
    // #605 — el arranque se MIDE, no se estima.
    //
    // El CA es «arranque percibido ≤ 3 s», y percibido quiere decir hasta que
    // hay algo en pantalla con lo que se puede interactuar. Medirlo a ojo con
    // un cronómetro da números que no se pueden comparar entre máquinas ni
    // entre versiones; imprimirlo lo vuelve un dato que cualquiera reproduce.
    //
    // Va antes que todo lo demás porque el reloj tiene que incluir leer la
    // configuración y armar la ventana: son parte de lo que el jugador espera.
    let startup_started = std::time::Instant::now();

    // #588 — la configuracion ahora se ESCRIBE mientras Play corre: el wizard
    // de primer arranque es su primer escritor. Un `Arc<Config>` inmutable
    // obligaria a inventar un camino paralelo para lo que el wizard cambia
    // (idioma, carpetas, core) y a tener dos verdades sobre la misma cosa.
    let cfg = Arc::new(Mutex::new(config::Config::load()));

    // Un pánico deja ARCHIVO, no sólo una ventana que desaparece.
    //
    // Play se cierra de golpe y no queda nada: en una build de ventana no hay
    // consola donde caiga el mensaje, Windows no registra un evento (el unwind
    // es una salida ordenada del proceso) y el jugador sólo puede decir «se
    // cerró». Eso no se puede diagnosticar ni reproducir.
    //
    // Se instala ACÁ, apenas hay configuración: antes no se sabe dónde
    // escribir, y más tarde deja sin cubrir justo el arranque.
    install_panic_hook(&cfg.lock().unwrap().diagnostics_dir());

    // #621 — el nivel del log, antes de la primera línea que valga la pena.
    logging::configure(&cfg.lock().unwrap().log);

    // #585 — el idioma, ANTES de construir la ventana: Slint resuelve sus
    // `@tr` al armar los textos y no repinta lo ya creado.
    let cfg0 = cfg.lock().unwrap().clone();
    let lang = i18n::init(&cfg0.language);
    eprintln!("[play-ce] idioma:   {lang}");

    // #315: instalar y revertir un pack que llegó por fuera. En CE ése es el
    // ÚNICO camino —no hay descarga—, así que el CLI no es una herramienta de
    // mantenimiento al costado: es la mitad de cómo entran los packs, y la
    // otra mitad es la pantalla que lo hace con el mando desde el sillón.
    if let Some(code) = run_pack_cli(&cfg0) {
        std::process::exit(code);
    }

    log_info!("play", "carpeta de ROMs: {}", cfg0.rom_dir.display());

    let dat: Arc<Mutex<Option<Arc<nointro::DatFile>>>> = Arc::new(Mutex::new(None));
    let store = catalog::Store::open_deferred(&cfg0.catalog_path()).expect("catalog worker");
    let shutdown_store = store.clone();
    let lib = Arc::new(Mutex::new(library::Library::deferred(store)));
    // #593 — el estado de la VISTA de la biblioteca.
    //
    // `todos` es la copia viva de la biblioteca; el modelo de Slint es una
    // proyección suya. Existe porque filtrar tiene que poder recalcularse en
    // cada tecla sin volver a leer la caché ni pedirle el lock a `lib`, que
    // también lo usa el escaneo.
    //
    // `filtro` NO se persiste: el CA dice «los filtros persisten por sesión». Un
    // filtro guardado es una trampa — al día siguiente faltan tres cuartos de la
    // biblioteca y nada en pantalla explica por qué.
    // #595 — el `build_id` del core, sondeado UNA vez por sesión.
    //
    // Es lo que separa «compatible» de «no verificado»: sin él, `compat_grade`
    // no puede cerrar el caso y contesta `Experimental` para TODO pack, que es
    // honesto pero deja el semáforo en un solo color. Se sondea una vez porque
    // el sondeo lanza un proceso (#589) y el dato es del CORE, no del juego —
    // hacerlo por tarjeta serían catorce procesos por escaneo.
    //
    // `None` mientras no terminó, y `None` también si falló: los dos casos
    // significan lo mismo para el validador —no se pudo comprobar— y forzar un
    // valor inventado sería peor que no contestar.
    pack_cache::init(lib.lock().unwrap().store());
    let core_build: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let all_items: Arc<Mutex<Vec<library::Game>>> = Arc::new(Mutex::new(Vec::new()));
    let all_items_for_details = Arc::clone(&all_items);
    let filter = Arc::new(Mutex::new(library_view::Filter::default()));
    let order = Arc::new(Mutex::new(library_view::Order::Alphabetical));

    // #591 — UN token para toda la app, no uno por escaneo: el botón «Cancelar»
    // se cablea una sola vez al arrancar y tiene que poder cortar el escaneo que
    // esté corriendo, sea cual sea. `do_scan` lo baja a `false` al empezar.
    let scan_cancel: library::CancelToken = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Holds the path to a downloaded launcher update (`<exe>.new`) once it is
    // ready to apply.  Filled by do_update_check(); read by on_update_apply().
    // #654 — «Configurar» y «Jugar» entran por el MISMO `launch_entry`, y desde
    // que Jugar saltea la pantalla previa hace falta distinguirlos: sin esto,
    // el botón que existe para cambiar la configuración sería el único que ya
    // no puede abrirla. Vale para UNA pulsación, como `partida-limpia`:
    // dejarla puesta haría que el siguiente Jugar volviera a preguntar.
    let force_configuration: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    // ---- Slint window -------------------------------------------------------
    let app = AppWindow::new().expect("Slint init failed");
    artwork::init(
        &app,
        config::Config::config_dir().join("cache").join("covers"),
    );
    app.set_active_tab(0);
    // #585 — recién acá: las traducciones empaquetadas se registran con el
    // primer componente (ver i18n::aplicar_a_slint).
    i18n::apply_to_slint();

    app.set_status(tr!("Cargando biblioteca…").into());
    {
        let (weak, lib, dat, all_items, filter, order, cfg) = (
            app.as_weak(),
            lib.clone(),
            dat.clone(),
            all_items.clone(),
            filter.clone(),
            order.clone(),
            cfg.clone(),
        );
        let legacy = cfg0.lib_path.clone();
        let dat_path = cfg0.dat_path.clone();
        std::thread::spawn(move || {
            let store = lib.lock().unwrap().store();
            let read_only = store.read_only().unwrap_or(true);
            CATALOG_WRITABLE.store(!read_only, Ordering::Release);
            let recovery_error = if read_only {
                None
            } else {
                pack_cache::global().and_then(|repo| repo.recover_promotions().err())
            };
            let result = lib.lock().unwrap().load(&legacy);
            if result.is_ok() {
                *all_items.lock().unwrap() = lib.lock().unwrap().get_all();
            }
            let weak_dat = weak.clone();
            let dat2 = dat.clone();
            // DAT loading does not delay the cached first view.
            std::thread::spawn(move || {
                *dat2.lock().unwrap() = dat_path
                    .as_deref()
                    .and_then(nointro::DatFile::load)
                    .map(Arc::new);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak_dat.upgrade()
                        && !read_only
                    {
                        app.invoke_rescan();
                    }
                });
            });
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    match result {
                        Ok(()) => {
                            refresh_ui(&app, &all_items, &filter, &order, &cfg);
                            app.set_status(recovery_error.unwrap_or_else(|| if read_only {
                                "Biblioteca en modo de solo lectura: las preferencias y las estadísticas no se guardarán.".into()
                            } else { tr!("Biblioteca cargada") }).into());
                            if !read_only {
                                app.invoke_rescan();
                            }
                        }
                        Err(error) => app
                            .set_status(format!("No se pudo abrir la biblioteca: {error}").into()),
                    }
                }
            });
        });
    }
    // #601: la cola de sesiones sin cerrar, de la más nueva a la más vieja.
    //
    // Se pregunta UNA vez por sesión y no una sola vez en total: si Play murió
    // dos veces, hay dos partidas sin dueño y decidir por una no decide por la
    // otra.
    fn render_orphaned_session(
        app: &AppWindow,
        orphaned_sessions: &Mutex<Vec<session::OrphanedSession>>,
    ) {
        let queue = orphaned_sessions.lock().unwrap();
        let Some(h) = queue.last() else {
            app.set_orphan_open(false);
            return;
        };
        app.set_orphan_text(
            tr!(
                "La última sesión de {} ({}) no registró su cierre. Play encontró su manifest.",
                rom_name(&h.manifest.rom),
                h.manifest.created
            )
            .as_str()
            .into(),
        );
        app.set_orphan_save(
            match &h.savestate {
                Some(_) => tr!("Savestate de salida"),
                None => String::new(),
            }
            .as_str()
            .into(),
        );
        app.set_orphan_open(true);
    }

    let orphaned_sessions: Arc<Mutex<Vec<session::OrphanedSession>>> = Arc::new(Mutex::new(
        session::orphaned_sessions(&cfg.lock().unwrap().sessions_dir()),
    ));

    // #588/#599 — el estado del wizard se declara ACÁ y no donde se abre: dos
    // filas de Ajustes lo abren en su paso (carpetas, core), así que tiene que
    // existir antes de cablear esa pantalla. Quién lo abre al arrancar sigue
    // decidiéndose más abajo, junto al resto del wizard.
    let onboarding_state = Arc::new(Mutex::new(onboarding::State::of(&cfg.lock().unwrap())));

    render_settings(&app, &cfg, &all_items);

    // #601 — y si quedó una sesión sin cerrar, se pregunta antes que nada:
    // es la única pantalla que habla de algo que ya pasó y que puede perderse.
    render_orphaned_session(&app, &orphaned_sessions);

    // #592 — la vista guardada manda desde el primer frame: aplicarla después,
    // al primer redibujo, haría parpadear la grilla en quien eligió lista.
    app.set_list_view(cfg0.list_view);
    app.on_view_changed({
        let cfg = Arc::clone(&cfg);
        move |list_view_enabled| {
            if let Ok(mut c) = cfg.lock()
                && c.list_view != list_view_enabled
            {
                c.list_view = list_view_enabled;
                c.save();
            }
        }
    });

    // ---- #599: Ajustes -------------------------------------------------------
    //
    // Los valores se re-arman al entrar y después de cada cambio. `refrescar`
    // no los toca porque no dependen de la biblioteca: mezclarlos ahí haría que
    // escribir en la búsqueda recalculara los ajustes.
    fn render_settings(
        app: &AppWindow,
        cfg: &Mutex<config::Config>,
        all_items: &Mutex<Vec<library::Game>>,
    ) {
        let c = cfg.lock().unwrap();
        // El nombre del idioma sale de `onboarding::nombre_idioma`, que es el
        // mismo que usan el wizard y su resumen. Estaba escrito otra vez acá y
        // ya decían cosas distintas: el wizard «Predeterminado» y Ajustes «Del
        // sistema», dos nombres para el mismo valor en dos pantallas que el
        // jugador ve una después de la otra.
        app.set_setting_language(onboarding::language_name(&c.language).as_str().into());
        let folders = config::rom_folders(&c);
        let n = all_items
            .lock()
            .map(|g| g.iter().filter(|x| !x.hidden).count())
            .unwrap_or(0);
        app.set_setting_folders(
            format!(
                "{} · {}",
                if folders.len() == 1 {
                    tr!("1 carpeta")
                } else {
                    tr!("{} carpetas", folders.len())
                },
                if n == 1 {
                    tr!("1 juego")
                } else {
                    tr!("{} juegos", n)
                }
            )
            .as_str()
            .into(),
        );

        // Los cores se cuentan por PLATAFORMA servida y no por archivo: el
        // wizard anota una entrada por cada consola que el core sirve (#589),
        // así que «3 cores» sobre un solo archivo sería mentir por tres.
        let files: std::collections::BTreeSet<&std::path::PathBuf> = c.cores.values().collect();
        app.set_setting_cores(
            format!(
                "{} · {}",
                if files.len() == 1 {
                    tr!("1 core")
                } else {
                    tr!("{} cores", files.len())
                },
                if c.cores.is_empty() {
                    tr!("ninguna consola cubierta")
                } else {
                    c.cores.keys().cloned().collect::<Vec<_>>().join(", ")
                }
            )
            .as_str()
            .into(),
        );

        // #599: la precedencia, a la vista. Hoy el perfil de salida se define
        // globalmente o lo recomienda el pack; los niveles por consola y por
        // juego existen en el modelo y se llenan cuando la ficha los ofrezca.
        let r = settings::resolve(
            None,
            None,
            // El NOMBRE, no el id: `crt` es lo que se guarda en el TOML y «CRT
            // simulado» es lo que se lee en el sillón.
            (!c.output_global.is_empty()).then(|| {
                launch_config::output_name(&c.output_global)
                    .map(str::to_string)
                    .unwrap_or_else(|| c.output_global.clone())
            }),
            None,
            tr!("Automático"),
        );
        app.set_setting_output(r.value.as_str().into());
        app.set_setting_output_origin(translate_origin(r.origin).as_str().into());

        // Los controles del juego, como estado: de fábrica o personalizados.
        app.set_setting_controls(
            if c.control_keys.is_empty() && c.control_buttons.is_empty() {
                tr!("De fábrica: flechas + Z X A S · el mando tal cual")
            } else {
                tr!("Personalizados")
            }
            .as_str()
            .into(),
        );

        // #678 — la carpeta de packs, como estado. Sin carpeta no dice «No»
        // a secas: dice qué packs SÍ valen, que es lo que la fila contesta.
        app.set_setting_packs(
            match c.pack_dirs.first() {
                None => tr!("Sin carpeta — valen los instalados y los que estén junto a la ROM"),
                Some(d) => {
                    let n: usize = c.pack_dirs.iter().map(|d| explorer::count_packs(d)).sum();
                    format!(
                        "{} · {}",
                        d.display(),
                        if n == 1 {
                            tr!("1 pack")
                        } else {
                            tr!("{} packs", n)
                        }
                    )
                }
            }
            .as_str()
            .into(),
        );

        app.set_setting_data(
            c.saves_dir
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
                .as_str()
                .into(),
        );
    }

    let pending_destructive: Arc<Mutex<Option<settings::Destructive>>> = Arc::new(Mutex::new(None));
    let pending_pack_removal = Arc::new(Mutex::new(None::<std::path::PathBuf>));

    // #678 — el diálogo de packs. La máquina vive en `install.rs`; esto es el
    // espejo que los callbacks comparten.
    let install_state: Arc<Mutex<install::State>> = Arc::new(Mutex::new(install::State::default()));

    // El diálogo de controles del juego, y la copia de trabajo de lo elegido.
    // La copia se guarda EN CADA asignación —como el resto de Ajustes— y se
    // recarga de la config al abrir: dos fuentes de verdad es cómo el diálogo
    // muestra una cosa y el juego arranca con otra.
    let controls_st: Arc<Mutex<(controls::Dialog, controls::Controls)>> =
        Arc::new(Mutex::new(Default::default()));
    // El modo captura del hilo del mando: mientras está puesto, el próximo
    // botón con nombre viaja crudo al diálogo en vez de navegar.
    let button_capture: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    app.on_setting_activate({
        let weak = app.as_weak();
        let cfg = Arc::clone(&cfg);
        let destructive_open = Arc::clone(&pending_destructive);
        let inst = Arc::clone(&install_state);
        let ctl = Arc::clone(&controls_st);
        let all_items = Arc::clone(&all_items);
        let onboarding_state = Arc::clone(&onboarding_state);
        let filter = Arc::clone(&filter);
        let order = Arc::clone(&order);
        move |row| {
            let Some(app) = weak.upgrade() else { return };
            let Some(action) = settings::action(row) else {
                return;
            };
            match action {
                settings::Action::Info => {}

                // El idioma se aplica EN CALIENTE: Slint re-evalúa sus `@tr` y
                // los textos que arma Rust se vuelven a pintar acá abajo. Sin
                // eso, el jugador elige inglés y la pantalla que está mirando
                // sigue en español hasta el próximo arranque.
                settings::Action::CycleLanguage => {
                    let language = {
                        let mut c = cfg.lock().unwrap();
                        c.language = onboarding::cycle_language(&c.language, 1);
                        c.save();
                        c.language.clone()
                    };
                    i18n::change_language(&language);
                    i18n::apply_to_slint();
                    // `aplicar_a_slint` alcanza a los `@tr` de la UI, pero NO a
                    // los textos que compone Rust y deja puestos en una
                    // propiedad: el contador de la barra, el «Jugado: …» de
                    // Inicio, las filas de esta misma pantalla. Sin volver a
                    // armarlos, el jugador cambia a inglés y le queda media
                    // pantalla en español, que se lee como que el cambio falló.
                    refresh_ui(&app, &all_items, &filter, &order, &cfg);
                    let n = all_items
                        .lock()
                        .map(|g| g.iter().filter(|x| !x.hidden).count())
                        .unwrap_or(0);
                    app.set_status(
                        if n == 1 {
                            tr!("{} juego en la biblioteca", n)
                        } else {
                            tr!("{} juegos en la biblioteca", n)
                        }
                        .into(),
                    );
                    render_settings(&app, &cfg, &all_items);
                }

                settings::Action::CycleOutput => {
                    {
                        let mut c = cfg.lock().unwrap();
                        c.output_global = launch_config::cycle_global_output(&c.output_global, 1);
                        c.save();
                    }
                    render_settings(&app, &cfg, &all_items);
                }

                settings::Action::EditFolders | settings::Action::EditCores => {
                    let step = if action == settings::Action::EditFolders {
                        onboarding::Step::Folders
                    } else {
                        onboarding::Step::Core
                    };
                    // Se re-arma desde la configuración de AHORA: el estado con
                    // el que CE arrancó puede tener carpetas que el jugador ya
                    // quitó, y editar sobre una lista vieja las devolvería.
                    {
                        let c = cfg.lock().unwrap();
                        let mut e = onboarding::State::of(&c);
                        e.step = step;
                        e.single_step_mode = true;
                        *onboarding_state.lock().unwrap() = e;
                    }
                    app.set_onboarding_row(0);
                    app.set_onboarding_terms_version(onboarding::TERMS_VERSION.into());
                    refresh_onboarding(&app, &onboarding_state.lock().unwrap());
                    app.set_onboarding_open(true);
                }

                // #620 — el export es un ZIP normal: se abre con el explorador,
                // se copia a un pendrive y se importa en otra máquina sin el
                // launcher de por medio.
                //
                // En CE no es una comodidad. Es la ÚNICA copia de respaldo que
                // existe, porque no hay nube que la haga sola: un jugador que
                // no puede sacar sus partidas de esta máquina las tiene en un
                // solo disco y no lo sabe.
                settings::Action::Export => {
                    let (saves, data_dir) = {
                        let c = cfg.lock().unwrap();
                        (c.saves_dir.clone(), c.sessions_dir())
                    };
                    let destination =
                        data_dir.join(format!("ayther-datos-{}.zip", date::compact_timestamp()));
                    let cfg_file = config::Config::config_file();
                    app.set_toast_text(
                        match porting::export(&saves, Some(&cfg_file), &destination) {
                            Ok(r) => tr!(
                                "Listo: {} guardado(s) en {}",
                                r.saves,
                                r.file.display().to_string()
                            ),
                            Err(e) => {
                                log_error!("play", "exportando: {e}");
                                tr!("No se pudo exportar")
                            }
                        }
                        .as_str()
                        .into(),
                    );
                }

                settings::Action::OpenControls => {
                    let mut st = ctl.lock().unwrap();
                    // La copia de trabajo sale de la CONFIG DE AHORA: lo que
                    // se guardó desde cualquier lado es lo que se muestra.
                    let c = cfg.lock().unwrap();
                    st.1 = controls::Controls::of(&c.control_keys, &c.control_buttons);
                    drop(c);
                    st.0.open();
                    render_controls(&app, &st.0, &st.1);
                }

                // #678 — los dos caminos del pack abren el MISMO diálogo, en
                // su modo. Designar la carpeta arranca por la actual —volver a
                // elegir es corregir, no empezar de cero— e instalar por donde
                // el diálogo se quedó, o la home.
                settings::Action::ChoosePacksFolder => {
                    let source = cfg
                        .lock()
                        .unwrap()
                        .pack_dirs
                        .first()
                        .cloned()
                        .unwrap_or_else(explorer::starting_path);
                    let mut e = inst.lock().unwrap();
                    e.open(install::Mode::Folder, source);
                    render_install(&app, &e);
                }
                settings::Action::InstallPack => {
                    let mut e = inst.lock().unwrap();
                    e.open(install::Mode::File, explorer::starting_path());
                    render_install(&app, &e);
                }

                settings::Action::Confirm(d) => {
                    *destructive_open.lock().unwrap() = Some(d);
                    app.set_confirm_title(d.title().into());
                    app.set_confirm_keeps(d.preserved_items().into());
                    app.set_confirm_action(d.confirm().into());
                    app.set_confirm_open(true);
                }
            }
        }
    });

    // ---- #678: el diálogo de packs ------------------------------------------
    //
    // El reparto es el de siempre: `install.rs` decide, esto traduce a Slint.
    // El único trabajo de verdad —auditar e instalar— va en su hilo, porque
    // sobre un `.ay` de cientos de MB la auditoría se siente y la ventana no
    // puede congelarse con cara de colgada.
    app.on_install_move({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        move |d| {
            let Some(app) = weak.upgrade() else { return };
            let mut e = inst.lock().unwrap();
            e.move_by(d);
            app.set_install_row(e.row);
        }
    });

    app.on_install_activate({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        let cfg = Arc::clone(&cfg);
        let all_items = Arc::clone(&all_items);
        let lib = Arc::clone(&lib);
        let filter = Arc::clone(&filter);
        let order = Arc::clone(&order);
        move |row| {
            let Some(app) = weak.upgrade() else { return };
            let selected = {
                let mut e = inst.lock().unwrap();
                e.row = row;
                let r = e.activate();
                render_install(&app, &e);
                r
            };
            let Some(path) = selected else { return };

            let cover_game = inst.lock().unwrap().cover_game.clone();
            if let Some(rom) = cover_game {
                let (weak, inst, lib, all_items, filter, order, cfg) = (
                    weak.clone(),
                    inst.clone(),
                    lib.clone(),
                    all_items.clone(),
                    filter.clone(),
                    order.clone(),
                    cfg.clone(),
                );
                std::thread::spawn(move || {
                    let result = (|| -> Result<library::Game, String> {
                        image::ImageReader::open(&path)
                            .map_err(|e| e.to_string())?
                            .with_guessed_format()
                            .map_err(|e| e.to_string())?
                            .decode()
                            .map_err(|_| tr!("No se pudo abrir la imagen. Elegí otra carátula."))?;
                        let mut library = lib.lock().unwrap();
                        let mut game = library
                            .known_games()
                            .get(&rom)
                            .cloned()
                            .ok_or_else(|| tr!("El juego ya no está en la biblioteca."))?;
                        game.user_art = Some(path.to_string_lossy().into_owned());
                        library.save_game(&game)?;
                        Ok(game)
                    })();
                    let _ = slint::invoke_from_event_loop(move || {
                        let Some(app) = weak.upgrade() else { return };
                        let mut state = inst.lock().unwrap();
                        state.phase = install::Phase::Browsing;
                        match result {
                            Ok(game) => {
                                if let Some(current) = all_items
                                    .lock()
                                    .unwrap()
                                    .iter_mut()
                                    .find(|g| g.id == game.id)
                                {
                                    *current = game;
                                }
                                state.is_open = false;
                                refresh_ui(&app, &all_items, &filter, &order, &cfg);
                            }
                            Err(error) => state.cover_error = error,
                        }
                        render_install(&app, &state);
                    });
                });
                return;
            }
            let packs = cfg.lock().unwrap().packs_install_dir.clone();
            let (weak2, inst2) = (weak.clone(), Arc::clone(&inst));
            INSTALL_CANCEL.store(false, Ordering::Release);
            app.set_install_stage(tr!("Copiando pack…").into());
            app.set_install_done(0);
            app.set_install_total(0);
            app.set_install_cancelable(true);
            std::thread::spawn(move || {
                // El MISMO camino que `--install`: staging, auditoría antes de
                // tocar nada, promoción atómica, generación anterior guardada.
                let id = install::State::id_from(&path);
                let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
                let r = pack_install::install_local_progress(
                    &packs,
                    &id,
                    &path,
                    &INSTALL_CANCEL,
                    &mut |stage, done, total| {
                        if last.elapsed() < std::time::Duration::from_millis(100) && done != total {
                            return;
                        }
                        last = std::time::Instant::now();
                        let (weak, stage) = (weak2.clone(), stage.to_owned());
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak.upgrade() {
                                app.set_install_cancelable(stage != "Finalizando…");
                                app.set_install_stage(stage.into());
                                app.set_install_done((done / 1024) as i32);
                                app.set_install_total((total / 1024) as i32);
                            }
                        });
                    },
                );
                let ok = matches!(r, pack_install::InstallResult::Installed { .. });
                // El nombre y la versión se leen del pack YA PROMOVIDO: es lo
                // que quedó instalado, no lo que el archivo elegido decía ser.
                let info = ok
                    .then(|| pack_meta::read(&pack_install::installed_path(&packs, &id)).1)
                    .flatten();
                let res = install::outcome_of(
                    &id,
                    info.as_ref().map(|i| i.name.as_str()).unwrap_or(""),
                    info.as_ref().map(|i| i.version.as_str()).unwrap_or(""),
                    &r,
                );
                slint::invoke_from_event_loop(move || {
                    let Some(app) = weak2.upgrade() else { return };
                    {
                        let mut e = inst2.lock().unwrap();
                        e.phase = install::Phase::Outcome(res);
                        render_install(&app, &e);
                    }
                    // La biblioteca se entera YA: el escaneo asocia el pack por
                    // el CRC que declara. Sin esto, «instalado» convive con una
                    // ficha que dice «sin pack» hasta el próximo arranque.
                    if ok {
                        app.invoke_rescan();
                    }
                })
                .ok();
            });
        }
    });

    app.on_install_action({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        let cfg = Arc::clone(&cfg);
        let all_items = Arc::clone(&all_items);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let selected = selected_install_source(&inst);
            let Some(d) = selected else { return };
            // REEMPLAZA, no acumula: la fila de Ajustes designa UNA carpeta y
            // eso es lo que se ve. Las demás se agregan a mano al TOML
            // (`pack_dirs` es lista) y el escaneo las lee todas.
            {
                let mut c = cfg.lock().unwrap();
                c.pack_dirs = vec![d];
                c.save();
            }
            render_install(&app, &inst.lock().unwrap());
            render_settings(&app, &cfg, &all_items);
            app.invoke_rescan();
        }
    });

    app.on_install_revert({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        let cfg = Arc::clone(&cfg);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut e = inst.lock().unwrap();
            // Sólo desde un resultado que ofreció volver: un Y suelto en medio
            // de la navegación no puede deshacer una instalación.
            let install::Phase::Outcome(r) = &e.phase else {
                return;
            };
            if !r.can_revert {
                return;
            }
            let packs = cfg.lock().unwrap().packs_install_dir.clone();
            let id = r.pack_id.clone();
            e.phase = install::Phase::Working;
            app.set_install_stage("Restaurando pack…".into());
            app.set_install_cancelable(false);
            render_install(&app, &e);
            drop(e);
            let (weak, inst) = (weak.clone(), inst.clone());
            std::thread::spawn(move || {
                let result = pack_install::rollback(&packs, &id);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak.upgrade() else { return };
                    let mut state = inst.lock().unwrap();
                    state.phase = install::Phase::Outcome(install::revert_outcome_of(&id, &result));
                    render_install(&app, &state);
                    drop(state);
                    app.invoke_rescan();
                });
            });
        }
    });

    app.on_install_close({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut e = inst.lock().unwrap();
            if matches!(e.phase, install::Phase::Working) {
                if !app.get_install_cancelable() {
                    return;
                }
                if app.get_install_cancelable() {
                    INSTALL_CANCEL.store(true, Ordering::Release);
                    app.set_install_stage(tr!("Cancelando…").into());
                }
                return;
            }
            if e.close() {
                render_install(&app, &e);
            }
        }
    });

    // ---- Controles del juego ------------------------------------------------
    //
    // Guardar pasa EN CADA asignación: es la conducta del resto de Ajustes, y
    // la que hace que cerrar el diálogo con B nunca pierda nada. El TOML del
    // contrato no se escribe acá — lo escribe el lanzamiento, fresco, con lo
    // que la config diga en ese momento.
    app.on_controls_move({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        move |d| {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            st.0.move_by(d);
            render_controls(&app, &st.0, &st.1);
        }
    });

    app.on_controls_capture_key({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            st.0.capture_key();
            render_controls(&app, &st.0, &st.1);
        }
    });

    app.on_controls_capture_button({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        let capture = Arc::clone(&button_capture);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            st.0.capture_button();
            // El hilo del mando entra en modo captura SÓLO si el diálogo lo
            // aceptó (una dirección no captura): dos verdades sobre el modo es
            // un botón que navega y asigna a la vez.
            capture.store(st.0.capture == controls::Capture::Button, Ordering::Relaxed);
            render_controls(&app, &st.0, &st.1);
        }
    });

    app.on_controls_key({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        let cfg = Arc::clone(&cfg);
        move |text| {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            let (d, c) = &mut *st;
            if d.key_received(c, &text) {
                let mut cf = cfg.lock().unwrap();
                cf.control_keys = c.keys.clone();
                cf.control_buttons = c.buttons.clone();
                cf.save();
            }
            render_controls(&app, &st.0, &st.1);
        }
    });

    app.on_controls_button({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        let cfg = Arc::clone(&cfg);
        move |button| {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            let (d, c) = &mut *st;
            if d.button_received(c, &button) {
                let mut cf = cfg.lock().unwrap();
                cf.control_keys = c.keys.clone();
                cf.control_buttons = c.buttons.clone();
                cf.save();
            }
            render_controls(&app, &st.0, &st.1);
        }
    });

    app.on_controls_restore({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        let cfg = Arc::clone(&cfg);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            st.1.restore();
            {
                let mut cf = cfg.lock().unwrap();
                cf.control_keys.clear();
                cf.control_buttons.clear();
                cf.save();
            }
            render_controls(&app, &st.0, &st.1);
        }
    });

    app.on_controls_close({
        let weak = app.as_weak();
        let ctl = Arc::clone(&controls_st);
        let capture = Arc::clone(&button_capture);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut st = ctl.lock().unwrap();
            // B en dos escalones: primero cancela una captura, después cierra.
            st.0.close();
            capture.store(false, Ordering::Relaxed);
            render_controls(&app, &st.0, &st.1);
        }
    });

    // #678 — Y sobre una fila de Ajustes: su secundaria, si la fila tiene una.
    // Quitar la carpeta NO borra nada —los `.ay` quedan donde están— y el
    // toast lo dice, porque «quitar» al lado de «packs» se lee como borrar.
    app.on_setting_secondary({
        let weak  = app.as_weak();
        let cfg   = Arc::clone(&cfg);
        let all_items = Arc::clone(&all_items);
        move |row| {
            let Some(app) = weak.upgrade() else { return };
            match settings::secondary_action(row) {
                Some(settings::SecondaryAction::RemovePacksFolder) => {
                    let had_custom_folder = {
                        let mut c = cfg.lock().unwrap();
                        let had_custom_folder = !c.pack_dirs.is_empty();
                        c.pack_dirs.clear();
                        c.save();
                        had_custom_folder
                    };
                    if had_custom_folder {
                        app.set_toast_text(tr!(
                            "La carpeta dejó de mirarse. Tus packs quedan donde están; los instalados no cambian."
                        ).as_str().into());
                        render_settings(&app, &cfg, &all_items);
                        app.invoke_rescan();
                    }
                }
                None => {}
            }
        }
    });

    // ---- #601: modo seguro, huérfanas y doble lanzamiento -------------------
    //
    // El modo seguro NO tiene su propio camino de lanzamiento: arma el
    // `PendingLaunch` con la config segura y usa el mismo `config_accept` que
    // todo lo demás. Un segundo camino sería un segundo lugar donde se escribe
    // el manifest y se toma el lock, y el que se olvide de uno de los dos va a
    // ser justo el que corre después de un crash.
    app.on_crash_safe_mode({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            // El banner ANTES de lanzar: si se pusiera al volver, el jugador
            // vería su juego sin el pack sin saber por qué.
            app.set_safe_mode(true);
            app.invoke_config_accept();
        }
    });

    app.on_exit_safe_mode({
        let weak = app.as_weak();
        move || {
            if let Some(app) = weak.upgrade() {
                app.set_safe_mode(false);
            }
        }
    });

    app.on_orphan_discard({
        let weak = app.as_weak();
        let orphaned_sessions = Arc::clone(&orphaned_sessions);
        move || {
            let Some(app) = weak.upgrade() else { return };
            // Borra la carpeta de ESA sesión. La ROM y los guardados viven en
            // otro árbol y `session::descartar` no puede alcanzarlos — se
            // niega, incluso, si lo que le pasan no tiene `launch.toml`.
            if let Some(h) = orphaned_sessions.lock().unwrap().pop()
                && let Err(e) = session::discard(&h)
            {
                eprintln!("[play] no se pudo descartar la sesión {}: {e}", h.id);
            }
            render_orphaned_session(&app, &orphaned_sessions);
        }
    });

    app.on_orphan_recover({
        let weak = app.as_weak();
        let orphaned_sessions = Arc::clone(&orphaned_sessions);
        move || {
            let Some(app) = weak.upgrade() else { return };
            // Recuperar CIERRA la sesión y conserva lo que dejó. El savestate
            // queda donde está —volver a él es #604— y lo que se resuelve acá
            // es que deje de estar sin dueño: sin el cierre, el mismo diálogo
            // volvería a aparecer en cada arranque.
            if let Some(h) = orphaned_sessions.lock().unwrap().pop() {
                let _ = session::close(
                    &h.dir,
                    &session::SessionExit {
                        ended_at: date::now_utc(),
                        clean_exit: false,
                        exit_code: None,
                        savestate: h
                            .savestate
                            .as_ref()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        elapsed_seconds: 0,
                    },
                );
                app.set_status(tr!("Sesión recuperada: {}", h.id).as_str().into());
            }
            render_orphaned_session(&app, &orphaned_sessions);
        }
    });

    app.on_confirm_accept({
        let weak = app.as_weak();
        let cfg = Arc::clone(&cfg);
        let pending = Arc::clone(&pending_destructive);
        let pack_removal = Arc::clone(&pending_pack_removal);
        let lib = Arc::clone(&lib);
        let games = Arc::clone(&all_items);
        let filter = Arc::clone(&filter);
        let order = Arc::clone(&order);
        move || {
            let Some(app) = weak.upgrade() else { return };
            // #620 — se ejecuta LA QUE SE CONFIRMÓ, y no «la de siempre». Hoy
            // en CE hay una sola destructiva cableada, pero el tipo ya tiene
            // tres: leer el pendiente es lo que hace que agregar la segunda no
            // sea el día en que el diálogo empieza a hacer otra cosa que la que
            // el jugador aceptó.
            let Some(pending_action) = pending.lock().unwrap().take() else {
                return;
            };
            match pending_action {
                settings::Destructive::RemovePack => {
                    let Some(path) = pack_removal.lock().unwrap().take() else {
                        return;
                    };
                    let packs = cfg.lock().unwrap().packs_install_dir.clone();
                    app.invoke_details_cancel();
                    app.set_details_pack_busy(true);
                    let weak = app.as_weak();
                    let (lib, games, filter, order, cfg) = (
                        Arc::clone(&lib), Arc::clone(&games), Arc::clone(&filter),
                        Arc::clone(&order), Arc::clone(&cfg),
                    );
                    std::thread::spawn(move || {
                        let result = pack_install::uninstall_path(&packs, &path);
                        let removed = matches!(result,
                            pack_install::UninstallResult::Uninstalled { .. }
                            | pack_install::UninstallResult::NotInstalled);
                        let mut message = match result {
                            pack_install::UninstallResult::Uninstalled { .. } => {
                                tr!("Pack desinstalado.")
                            }
                            pack_install::UninstallResult::NotInstalled => {
                                tr!("El pack ya no está en el disco.")
                            }
                            pack_install::UninstallResult::Io(error) => {
                                tr!("No se pudo desinstalar el pack: {}", error)
                            }
                        };
                        if removed {
                            let saved = lib.lock().unwrap().forget_pack(&path.to_string_lossy());
                            if let Err(error) = saved {
                                message = format!("El pack ya no está en el disco, pero no se pudo actualizar la biblioteca: {error}");
                            }
                            if let Some(repo) = pack_cache::global()
                                && let Err(error) = repo.invalidate(&path)
                            {
                                log_error!("play", "no se pudo invalidar el pack desinstalado: {error}");
                            }
                        }
                        let _ = weak.upgrade_in_event_loop(move |app| {
                            app.set_details_pack_busy(false);
                            app.set_toast_text(message.into());
                            if removed {
                                let latest = lib.lock().unwrap().get_all();
                                *games.lock().unwrap() = latest;
                                refresh_ui(&app, &games, &filter, &order, &cfg);
                                refresh_open_details(&app, &games, &cfg);
                            }
                            // Refresh after errors too, to show what remains on disk.
                            app.invoke_rescan();
                        });
                    });
                }
                // Rehacer la bienvenida NO borra nada: baja la bandera y el
                // wizard vuelve a aparecer con todo lo demás intacto.
                settings::Destructive::RedoWelcome => {
                    if let Ok(mut c) = cfg.lock() {
                        c.onboarding_complete = false;
                        c.save();
                    }
                    app.set_onboarding_open(true);
                }

                // Las que todavía no arma ninguna pantalla. Llegar acá sería
                // un botón sin su acción: mejor no hacer nada que hacer otra
                // cosa.
                other_action => {
                    log_error!("play", "confirmación sin acción: {other_action:?}")
                }
            }
        }
    });

    // ---- #593: filtros, orden y búsqueda ------------------------------------
    //
    // Todos los controles entran por UN callback. Podrían ser cinco —uno por
    // chip, uno por el select, uno por la búsqueda— pero los cinco terminan
    // haciendo lo mismo, y con uno solo es imposible que a alguno se le olvide
    // re-proyectar la lista.
    app.on_filter_changed({
        let weak = app.as_weak();
        let (all_items, filter, order, cfg) = (
            Arc::clone(&all_items),
            Arc::clone(&filter),
            Arc::clone(&order),
            Arc::clone(&cfg),
        );
        move || {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut f = filter.lock().unwrap();
                f.text = app.get_search().to_string();
                f.platform = app.get_filter_platform().to_string();
                f.compatible_only = app.get_filter_compatible();
                f.favorites_only = app.get_filter_favorites();
                f.show_hidden = app.get_show_hidden();
            }
            *order.lock().unwrap() = library_view::Order::from_index(app.get_order());
            refresh_ui(&app, &all_items, &filter, &order, &cfg);
        }
    });

    // ---- #597: la ficha de juego --------------------------------------------
    //
    // Se arma UNA vez, al abrir, y no al pintar: leer el pack, validarlo y
    // formatear tres textos por frame sería trabajo por cada movimiento de la
    // cruceta. Es la misma decisión que ya tomó #312 para la ficha de pack, por
    // el mismo motivo.
    let details_cancel = Arc::new(Mutex::new(Arc::new(AtomicBool::new(false))));
    app.on_details_cancel({
        let token = details_cancel.clone();
        let weak = app.as_weak();
        move || {
            token.lock().unwrap().store(true, Ordering::Release);
            if let Some(app) = weak.upgrade() {
                app.set_details_activate_pack_path("".into());
                if app.get_pack_checking() {
                    app.set_pack_audit("Comprobación cancelada".into());
                }
                app.set_pack_checking(false);
            }
        }
    });
    app.on_open_details({
        let weak = app.as_weak();
        let cfg = Arc::clone(&cfg);
        let details_cancel = details_cancel.clone();
        let details_lib = lib.clone();
        let details_build = core_build.clone();
        let details_items = all_items.clone();
        let details_filter = filter.clone();
        let details_order = order.clone();
        move |entry| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_details_activate_pack_path() != entry.pack_path
                || app.get_details_game().path != entry.path
            {
                app.set_details_activate_pack_path("".into());
            }
            if PREPARING.load(Ordering::Acquire) {
                PREPARE_CANCEL.store(true, Ordering::Release);
            }
            let token = Arc::new(AtomicBool::new(false));
            details_cancel
                .lock()
                .unwrap()
                .store(true, Ordering::Release);
            *details_cancel.lock().unwrap() = token.clone();
            let rom = std::path::PathBuf::from(entry.path.as_str());
            let pack = (!entry.pack_path.is_empty())
                .then(|| std::path::PathBuf::from(entry.pack_path.as_str()));
            let pack_ref = pack.clone();
            // #597 — NADA de leer el pack acá. El escaneo ya corrió el
            // validador para sacar el grado y guardó sus códigos; volver a
            // abrir un `.ay` de 138 MB en el hilo de la UI congelaba la ventana
            // varios segundos, que es como una ficha se convierte en un cuelgue.
            // Lo único que sigue necesitando el archivo es el nombre y la firma
            // del pack, y eso va en su hilo (más abajo).
            let (codes, has_pack, grade, reason, played, time, last) = {
                let g = all_items_for_details.lock().unwrap();
                match g.iter().find(|x| x.path == entry.path.as_str()) {
                    Some(j) => (
                        j.compat_codes.clone(),
                        j.has_pack,
                        j.pack_status,
                        j.compat_reason.clone(),
                        j.last_played > 0,
                        library_view::format_play_time(j.seconds_played),
                        if j.last_played > 0 {
                            date::from_epoch(j.last_played)
                        } else {
                            tr!("Nunca")
                        },
                    ),
                    None => (
                        Vec::new(),
                        false,
                        library::PackStatus::NoPack,
                        String::new(),
                        false,
                        library_view::format_play_time(0),
                        tr!("Nunca"),
                    ),
                }
            };
            // #642 — el eje del core sale del SONDEO. Se recalcula acá y no
            // se lee de `entry`: la ficha se abre después del arranque, y el
            // sondeo puede haber terminado entre que se armó el modelo y que el
            // jugador apretó A.
            let core = cores::cached_state(&cfg.lock().unwrap(), entry.platform.as_str());
            let e = game_details::axes(
                &codes,
                has_pack,
                grade,
                &reason,
                &core,
                entry.platform.as_str(),
            );

            app.set_details_game(entry.clone());
            render_details_pack_list(&app, &all_items_for_details.lock().unwrap());
            // A historical selection can outlive an uninstall; it is not a file
            // to inspect and must never appear among the installed options.
            let pack = pack.filter(|_| app.get_details_selected_pack_present());
            let profile = launch_config::config_path(
                &config::Config::config_dir(),
                &rom,
                pack_ref.as_deref(),
            );
            app.set_details_hd(launch_config::load(&profile).is_hd());
            // La línea técnica: consola · región · CRC. Sólo lo que se sabe —
            // un «·» seguido de nada se lee como un dato que se perdió.
            let mut technical_details = vec![entry.platform.to_string()];
            if !entry.region.is_empty() && entry.region != "UNK" {
                technical_details.push(entry.region.to_string());
            }
            if !entry.crc32.is_empty() {
                technical_details.push(format!("CRC {}", entry.crc32));
            }
            app.set_details_tech(technical_details.join(" · ").as_str().into());

            app.set_details_played(played);
            app.set_details_time(time.as_str().into());
            app.set_details_last_played(last.as_str().into());

            // #632 — dos preguntas y no tres ejes: «¿corre?», que la contesta
            // el core, y «¿está mejorado?», que la contesta el pack con la razón
            // de la ROM adentro. El eje de la ROM no desapareció: se mudó a
            // `motivo_del_pack`, que es de lo que siempre habló.
            app.set_details_status(e.core.state.as_int());
            // #642 — y el eje del core con su propio vocabulario, que dice más
            // que el grado: «Falta el core» y «El core no sirve» mandan a hacer
            // cosas distintas.
            app.set_details_core(core.as_int());
            app.set_details_status_reason(e.core.reason.as_str().into());
            app.set_details_pack_status(e.pack.state.as_int());
            app.set_details_pack_reason(e.pack_reason().into());
            // #631 — dos veredictos y no uno: el juego corre (lo dice el core) y
            // el pack se puede activar (lo dicen el pack y la ROM). Antes salía
            // uno solo de los tres ejes, así que un pack roto apagaba el botón
            // de jugar de un juego que corre perfecto sin él.
            app.set_details_playable(e.is_playable());
            app.set_details_pack_usable(e.pack_usable());
            app.set_details_pack_warning(e.short_pack_reason().into());
            // #633 — en qué está el pack y qué se puede hacer con él. La tabla
            // vive en `EstadoPack`, no acá: es la que los oráculos prueban sin
            // abrir una ventana.
            app.set_details_pack_usage_status(
                game_details::PackState::of(has_pack, e.pack_usable(), app.get_details_hd())
                    .as_int(),
            );

            // SIN PACK, CE NO TIENE A QUIÉN PREGUNTARLE QUÉ HAY PARA ESTA ROM.
            //
            // Play, acá, le pregunta al Hub: `/v1/roms/identificar` va del CRC
            // al juego y el catálogo ordena por compatibilidad. CE no habla con
            // ningún servicio (ADR-005), así que la ficha dice lo que sí sabe
            // —que el juego se juega igual, en su versión original— y ofrece el
            // único camino que hay: traer un `.ay` desde el disco (#678).
            //
            // Lo que NO hace es dejar el hueco de la pregunta con un «no se
            // pudo consultar»: eso se lee como una falla de red y manda a
            // reintentar algo que en esta edición no existe.
            app.set_details_no_pack(!has_pack);

            // #604 — de cuándo es lo que «Continuar» va a cargar. Sale del
            // perfil con el que se jugaría y se descarta si es de otra revisión
            // de la ROM: un savestate es una foto de la memoria y cargarlo
            // sobre otra revisión no da un error, da un juego que se comporta
            // raro media hora después.
            // #604 — el historial de este juego y perfil, para «03d».
            {
                let saves = cfg.lock().unwrap().saves_dir.clone();
                let gid = std::path::Path::new(entry.path.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let profile = if entry.has_pack && app.get_details_hd() {
                    "enhanced"
                } else {
                    saves::DEFAULT_PROFILE
                };
                let rows: Vec<SaveRow> = saves::versions(&saves, &gid, profile)
                    .into_iter()
                    .map(|v| SaveRow {
                        when: date::format_compact_timestamp(&v.when).as_str().into(),
                        // La revisión y el tamaño distinguen dos guardados de
                        // la misma tarde; sin ellos, tres filas con la misma
                        // fecha son tres filas iguales.
                        detail: format!(
                            "{} · {} KB",
                            if v.rom_crc32.is_empty() {
                                tr!("sin revisión")
                            } else {
                                tr!("Rev {}", v.rom_crc32)
                            },
                            v.bytes / 1024
                        )
                        .as_str()
                        .into(),
                    })
                    .collect();
                app.set_saves_list(ModelRc::new(VecModel::from(rows)));
                app.set_saves_row(0);
            }

            app.set_details_save(
                {
                    let saves = cfg.lock().unwrap().saves_dir.clone();
                    let gid = std::path::Path::new(entry.path.as_str())
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let profile = if entry.has_pack && app.get_details_hd() {
                        "enhanced"
                    } else {
                        saves::DEFAULT_PROFILE
                    };
                    saves::last(&saves, &gid, profile)
                        .filter(|v| v.is_compatible_with(entry.crc32.as_str()))
                        .map(|v| tr!("Guardado {}", date::format_compact_timestamp(&v.when)))
                        .unwrap_or_default()
                }
                .as_str()
                .into(),
            );

            // El nombre, el autor y la AUDITORÍA del pack salen del archivo, y
            // eso puede tardar: auditar es leer cada entrada que declara el
            // `integrity.toml` y volver a hashearla. Va en su hilo y la ficha se
            // abre YA, con la fila del pack diciendo que está leyendo. Bloquear
            // para mostrar un nombre sería cambiar «abre al instante» por «abre
            // exacto», y en una pantalla que se abre con A la primera gana.
            //
            // Lo que la fila NO dice es de quién es el pack. CE no lo sabe y no
            // lo inventa: ver el comentario de `verifier.rs`.
            app.set_pack_name("".into());
            app.set_pack_author("".into());
            app.set_pack_audit("".into());
            app.set_pack_audit_ok(false);
            app.set_pack_checking(false);
            let (lib, weak2, build) = (
                details_lib.clone(),
                weak.clone(),
                details_build.lock().unwrap().clone(),
            );
            let rom_path = entry.path.to_string();
            let configuration = cfg.lock().unwrap().clone();
            let (items, filter, order, cfg_view) = (
                details_items.clone(),
                details_filter.clone(),
                details_order.clone(),
                cfg.clone(),
            );
            std::thread::spawn(move || {
                let result = (|| -> Result<_, String> {
                    let (_rom_guard, mut game) = library::prepare_rom(&lib, &rom, &token)?;
                    let details = if let Some(path) = &pack {
                        let repo =
                            pack_cache::global().ok_or("La caché todavía no está disponible")?;
                        let mut last =
                            std::time::Instant::now() - std::time::Duration::from_secs(1);
                        let snapshot = repo.inspect(path, &token, &mut |done, total| {
                            if last.elapsed() >= std::time::Duration::from_millis(100) {
                                last = std::time::Instant::now();
                                let (weak, token, rom) =
                                    (weak2.clone(), token.clone(), rom_path.clone());
                                let _ = slint::invoke_from_event_loop(move || {
                                    if !token.load(Ordering::Acquire)
                                        && let Some(app) = weak.upgrade()
                                        && app.get_details_game().path.as_str() == rom
                                    {
                                        app.set_pack_checking(true);
                                        app.set_pack_progress(if total == 0 {
                                            0.0
                                        } else {
                                            done as f32 / total as f32
                                        });
                                        app.set_pack_audit("Comprobando pack…".into());
                                    }
                                });
                            }
                        })?;
                        let context = ayther_core::pack_validate::SessionCtx {
                            rom_crc32: u32::from_str_radix(&game.crc32, 16).ok(),
                            platform: library::platform_id(&game.platform),
                            core_build_id: build.as_deref(),
                            ..Default::default()
                        };
                        let details = pack_details::from_snapshot(&snapshot, &context);
                        library::enrich_pack_status(
                            std::slice::from_mut(&mut game),
                            build.as_deref(),
                        );
                        Some(details)
                    } else {
                        None
                    };
                    if token.load(Ordering::Acquire) {
                        return Err("Comprobación cancelada".into());
                    }
                    // Merge derived results into the latest row, keeping player edits.
                    let mut library = lib.lock().unwrap();
                    if let Some(mut latest) = library.known_games().get(&game.path).cloned()
                        && latest.id == game.id
                        && latest.pack_path == game.pack_path
                    {
                        latest.pack_status = game.pack_status;
                        latest.compat_reason = game.compat_reason;
                        latest.compat_codes = game.compat_codes;
                        if !library.store().read_only()? {
                            library.save_game(&latest)?;
                        }
                        game = latest;
                    }
                    Ok((game, details))
                })();
                let _ = slint::invoke_from_event_loop(move || {
                    if token.load(Ordering::Acquire) {
                        return;
                    }
                    let Some(app) = weak2.upgrade() else { return };
                    if !app.get_details_open() || app.get_details_game().path.as_str() != rom_path {
                        return;
                    }
                    app.set_pack_checking(false);
                    match result {
                        Err(error) => {
                            app.set_pack_audit(error.as_str().into());
                            app.set_status(error.into());
                        }
                        Ok((game, details)) => {
                            if let Some(row) =
                                items.lock().unwrap().iter_mut().find(|g| g.id == game.id)
                            {
                                *row = game.clone();
                            }
                            refresh_ui(&app, &items, &filter, &order, &cfg_view);
                            app.set_details_game(game_entry(&game, &configuration));
                            if let Some(d) = details {
                                let core = cores::cached_state(&configuration, &game.platform);
                                let axes = game_details::axes(
                                    &game.compat_codes,
                                    game.has_pack,
                                    game.pack_status,
                                    &game.compat_reason,
                                    &core,
                                    &game.platform,
                                );
                                app.set_details_pack_status(axes.pack.state.as_int());
                                app.set_details_pack_reason(axes.pack_reason().into());
                                app.set_details_pack_usable(axes.pack_usable());
                                app.set_details_pack_warning(axes.short_pack_reason().into());
                                app.set_pack_name(d.name.as_str().into());
                                app.set_pack_author(format!("{} · {}", d.author, d.version).into());
                                app.set_pack_audit_ok(d.audit == verifier::Audit::Complete);
                                app.set_pack_audit(d.audit.word().into());
                                app.set_details_pack_usage_status(
                                    game_details::PackState::of(
                                        game.has_pack,
                                        axes.pack_usable(),
                                        app.get_details_hd(),
                                    )
                                    .as_int(),
                                );
                                if app.get_details_activate_pack_path().as_str() == game.pack_path {
                                    app.set_details_activate_pack_path("".into());
                                    if axes.pack_usable() && !app.get_details_hd() {
                                        app.invoke_details_toggle_hd();
                                    }
                                }
                            }
                        }
                    }
                });
            });
            let _ = &cfg;
            app.set_details_open(true);
        }
    });

    // #604 — «Partida nueva» pasa por el MISMO camino, con la bandera puesta.
    //
    // No pisa nada: desde #603 cada salida escribe una versión nueva, así que
    // empezar de cero no puede borrar la partida anterior aunque el jugador la
    // abandone en el primer nivel. Eso es lo que hace que ofrecerla sea seguro.
    // #604 — restaurar AGREGA. La versión que estaba en curso sigue en el
    // historial, con su fecha: es lo que promete el diálogo, y es lo que hace
    // que restaurar no pueda salir mal.
    app.on_saves_restore({
        let weak = app.as_weak();
        let cfg = Arc::clone(&cfg);
        move |row| {
            let Some(app) = weak.upgrade() else { return };
            if row <= 0 {
                return;
            }
            let entry = app.get_details_game();
            let saves = cfg.lock().unwrap().saves_dir.clone();
            let gid = std::path::Path::new(entry.path.as_str())
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let profile = if entry.has_pack && app.get_details_hd() {
                "enhanced"
            } else {
                saves::DEFAULT_PROFILE
            };

            // Se vuelve a listar en vez de guardar las rutas en el modelo: el
            // disco pudo cambiar desde que se abrió la ficha, y una ruta vieja
            // restauraría un archivo que ya no está.
            let v = saves::versions(&saves, &gid, profile);
            let Some(selected) = v.get(row as usize) else {
                return;
            };
            match saves::restore(&saves, &gid, profile, selected, &date::compact_timestamp()) {
                Ok(_) => {
                    app.set_status(
                        tr!(
                            "Restaurado el guardado de {}",
                            date::format_compact_timestamp(&selected.when)
                        )
                        .as_str()
                        .into(),
                    );
                    app.set_saves_open(false);
                    // La ficha se re-arma: «Continuar» ahora apunta a otra
                    // partida y decirlo mal sería peor que no decirlo.
                    app.invoke_open_details(entry);
                }
                Err(e) => {
                    eprintln!("[play] no se pudo restaurar: {e}");
                    app.set_status(tr!("No se pudo restaurar el guardado.").into());
                }
            }
        }
    });

    app.on_details_play_clean({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let game = app.get_details_game();
            app.set_clean_run(true);
            app.set_details_open(false);
            app.invoke_launch_entry(game);
        }
    });

    app.on_details_play({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let game = app.get_details_game();
            app.set_clean_run(false);
            app.set_details_open(false);
            app.invoke_launch_entry(game);
        }
    });
    app.on_details_play_original({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut game = app.get_details_game();
            game.has_pack = false;
            game.pack_path = "".into();
            app.invoke_details_cancel();
            app.set_clean_run(false);
            app.set_details_open(false);
            app.invoke_launch_entry(game);
        }
    });
    // #598/#633 — poner o sacar el pack. Escribe la configuración de ESTA
    // combinación ROM+pack y no lanza: es reversible y se puede mover varias
    // veces antes de jugar. Lo que fija es el perfil que va a ir al manifest de
    // la próxima sesión — la ROM y los guardados no se tocan en ninguna de las
    // dos posiciones (§6), y el pack en disco tampoco.
    //
    // #633 — un pack que no sirve NO se activa. Antes alcanzaba con que el
    // archivo existiera, así que el atajo del mando podía poner un pack que la
    // ficha mostraba como inusable — y la sesión lo iba a dejar afuera igual.
    app.on_details_toggle_hd({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let e = app.get_details_game();
            if !e.has_pack {
                return;
            } // sin pack no hay nada que activar
            if !app.get_details_pack_usable() {
                return;
            }
            let rom = std::path::PathBuf::from(e.path.as_str());
            let pack = std::path::PathBuf::from(e.pack_path.as_str());
            let dir = config::Config::config_dir();
            let f = launch_config::config_path(&dir, &rom, Some(&pack));

            let caps = pack_meta::capabilities(&pack);
            let mut cfg = launch_config::load(&f);
            cfg.clamp_to(&caps);
            let want_hd = !cfg.is_hd();
            cfg.set_hd(want_hd, &caps);
            launch_config::save(&f, &cfg);
            app.set_details_hd(want_hd);
            // Y la sección lo dice de inmediato: el estado y su verbo salen de
            // la misma tabla, así que se recalculan acá y no al reabrir.
            app.set_details_pack_usage_status(
                game_details::PackState::of(true, true, want_hd).as_int(),
            );
        }
    });

    // #633 — la acción de la sección PACK. Es UNA por estado: activar, sacar, o
    // ir a buscar uno. Acá sólo se enruta; qué acción corresponde a cada estado
    // lo decide `EstadoPack`, que se prueba sin ventana.
    app.on_details_pack_act({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        move || {
            let Some(app) = weak.upgrade() else { return };
            match app.get_details_pack_usage_status() {
                1 | 2 => app.invoke_details_toggle_hd(),
                // #678 — sin pack, el botón abre el diálogo de instalar SOBRE
                // la ficha: el juego sigue detrás, y al cerrar se vuelve donde
                // se estaba. En AYTHER Play este estado lleva al Hub.
                0 => {
                    let mut e = inst.lock().unwrap();
                    e.open(install::Mode::File, explorer::starting_path());
                    render_install(&app, &e);
                }
                // No se puede activar. Las acciones de gestión son independientes.
                _ => {}
            }
        }
    });

    app.on_details_install_pack({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut e = inst.lock().unwrap();
            e.open(install::Mode::File, explorer::starting_path());
            render_install(&app, &e);
        }
    });

    app.on_details_toggle_pack({
        let weak = app.as_weak();
        move |path| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_scanning() || app.get_details_pack_busy() {
                return;
            }
            if path == app.get_details_game().pack_path {
                app.invoke_details_toggle_hd();
            } else {
                app.invoke_details_select_pack(path.clone());
                // Fulfil the explicit ON request only after the chosen pack's
                // inspection has completed for the still-open game.
                app.set_details_activate_pack_path(path);
            }
        }
    });

    app.on_details_select_pack({
        let weak = app.as_weak();
        let games = Arc::clone(&all_items);
        let lib = Arc::clone(&lib);
        let cfg = Arc::clone(&cfg);
        let filter = Arc::clone(&filter);
        let order = Arc::clone(&order);
        let core_build = Arc::clone(&core_build);
        move |pack_path| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_scanning() || app.get_details_pack_busy() {
                return;
            }
            app.set_details_activate_pack_path("".into());
            let entry = app.get_details_game();
            let mut selected = {
                let games = games.lock().unwrap();
                let Some(game) = games.iter().find(|g| g.path == entry.path.as_str()) else {
                    return;
                };
                if !game.available_packs.iter().any(|p| p == pack_path.as_str()) {
                    return;
                }
                let mut selected = game.clone();
                selected.pack_path = pack_path.to_string();
                selected.has_pack = true;
                selected
            };
            app.set_details_pack_busy(true);
            let build = core_build.lock().unwrap().clone();
            let (weak, games, lib, cfg, filter, order) = (
                weak.clone(),
                Arc::clone(&games),
                Arc::clone(&lib),
                Arc::clone(&cfg),
                Arc::clone(&filter),
                Arc::clone(&order),
            );
            std::thread::spawn(move || {
                // Grade before publishing: never pair the new path with the old verdict.
                library::enrich_pack_status(std::slice::from_mut(&mut selected), build.as_deref());
                let saved = lib.lock().unwrap().save_game(&selected);
                slint::invoke_from_event_loop(move || {
                    let Some(app) = weak.upgrade() else { return };
                    app.set_details_pack_busy(false);
                    if let Err(error) = saved {
                        app.set_status(format!("No se guardó la selección: {error}").into());
                        return;
                    }
                    let entry = {
                        let mut games = games.lock().unwrap();
                        let Some(game) = games.iter_mut().find(|g| g.path == selected.path) else {
                            return;
                        };
                        if !game.available_packs.contains(&selected.pack_path) {
                            return;
                        }
                        game.pack_path = selected.pack_path;
                        game.has_pack = true;
                        game.pack_status = selected.pack_status;
                        game.compat_reason = selected.compat_reason;
                        game.compat_codes = selected.compat_codes;

                        build_game_model_from(
                            std::slice::from_ref(game),
                            &[0],
                            &cfg.lock().unwrap(),
                        )
                        .row_data(0)
                    };
                    refresh_ui(&app, &games, &filter, &order, &cfg);
                    if let Some(entry) = entry
                        && app.get_details_open()
                        && app.get_details_game().path == entry.path
                    {
                        app.invoke_open_details(entry);
                    }
                })
                .ok();
            });
        }
    });

    app.on_details_next_pack({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let packs = app.get_details_packs();
            if packs.row_count() == 0 {
                return;
            }
            let selected = app.get_details_game().pack_path;
            let index = packs
                .iter()
                .position(|p| p.path == selected)
                .map_or(0, |i| (i + 1) % packs.row_count());
            if let Some(pack) = packs.row_data(index) {
                app.invoke_details_select_pack(pack.path);
            }
        }
    });

    app.on_details_uninstall_pack({
        let weak = app.as_weak();
        let games = Arc::clone(&all_items);
        let pending = Arc::clone(&pending_destructive);
        let pack_removal = Arc::clone(&pending_pack_removal);
        move |pack_path| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_scanning() || app.get_details_pack_busy() {
                return;
            }
            let entry = app.get_details_game();
            let games = games.lock().unwrap();
            let Some(game) = games.iter().find(|g| g.path == entry.path.as_str()) else {
                return;
            };
            if !game.available_packs.iter().any(|p| p == pack_path.as_str()) {
                return;
            }
            let path = std::path::PathBuf::from(pack_path.as_str());
            let action = settings::Destructive::RemovePack;
            app.set_confirm_title(i18n::gettext(action.title()).into());
            app.set_confirm_keeps(
                format!(
                    "{}\n\n{}",
                    path.display(),
                    i18n::gettext(action.preserved_items())
                )
                .into(),
            );
            app.set_confirm_action(i18n::gettext(action.confirm()).into());
            *pack_removal.lock().unwrap() = Some(path);
            *pending.lock().unwrap() = Some(action);
            app.set_confirm_open(true);
        }
    });

    app.on_details_configure({
        let weak = app.as_weak();
        let force_configuration = Arc::clone(&force_configuration);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let game = app.get_details_game();
            app.set_details_open(false);
            // La configuración previa sigue siendo la de #311: acá deja de ser
            // lo que A abre y pasa a ser una acción de la ficha, que es lo que
            // el diseño pide.
            //
            // #654 — y ahora es la ÚNICA puerta que la abre cuando el juego ya
            // se configuró alguna vez: Jugar pasa de largo.
            force_configuration.store(true, Ordering::Relaxed);
            app.invoke_launch_entry(game);
        }
    });

    // ---- Menú de juego (play-ce.pen «02d») ----------------------------------
    //
    // Las tres acciones toman el índice de la VISTA y lo traducen al de la
    // biblioteca. Traducirlo acá y no en Slint es deliberado: la vista cambia
    // con cada filtro, y un índice de vista guardado en la UI apunta a otro
    // juego en cuanto el filtro se mueve.
    //
    // Cada una escribe en `todos` Y en la caché. Escribir sólo en `todos` haría
    // que el favorito se perdiera al cerrar Play, y escribir sólo en la caché,
    // que no se viera hasta el próximo escaneo.
    let apply = {
        let (all_items, filter, order, cfg, lib) = (
            all_items.clone(),
            filter.clone(),
            order.clone(),
            cfg.clone(),
            lib.clone(),
        );
        move |weak: &slint::Weak<AppWindow>, visible_index: i32, f: &dyn Fn(&mut library::Game)| {
            let updated = {
                let games = all_items.lock().unwrap();
                let indices = library_view::visible_indices(
                    &games,
                    &filter.lock().unwrap(),
                    *order.lock().unwrap(),
                );
                indices.get(visible_index.max(0) as usize).map(|&index| {
                    let before = games[index].clone();
                    let mut game = before.clone();
                    f(&mut game);
                    (before, game)
                })
            };
            let Some((before, game)) = updated else {
                return;
            };
            let (weak, all_items, filter, order, cfg, lib) = (
                weak.clone(),
                all_items.clone(),
                filter.clone(),
                order.clone(),
                cfg.clone(),
                lib.clone(),
            );
            std::thread::spawn(move || {
                let result = lib.lock().unwrap().save_user_edit(&before, &game);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak.upgrade() else { return };
                    match result {
                        Ok(game) => {
                            if let Some(current) = all_items
                                .lock()
                                .unwrap()
                                .iter_mut()
                                .find(|g| g.id == game.id)
                            {
                                *current = game;
                            }
                            refresh_ui(&app, &all_items, &filter, &order, &cfg);
                        }
                        Err(error) => {
                            app.set_status(format!("No se guardó el cambio: {error}").into())
                        }
                    }
                });
            });
        }
    };
    app.on_game_favorite({
        let weak = app.as_weak();
        let apply = apply.clone();
        move |i| apply(&weak, i, &|g| g.favorite = !g.favorite)
    });
    app.on_game_hide({
        let weak = app.as_weak();
        let apply = apply.clone();
        move |i| apply(&weak, i, &|g| g.hidden = !g.hidden)
    });
    app.on_game_edit({
        let weak = app.as_weak();
        let apply = apply.clone();
        move |i, title| {
            let t = title.to_string();
            // Un título en blanco no se guarda como override: es alguien que
            // borró el campo, y guardarlo dejaría la tarjeta sin nombre.
            apply(&weak, i, &|g| {
                g.user_title = (!t.trim().is_empty()).then(|| t.trim().to_string());
            })
        }
    });
    app.on_game_restore({
        let weak = app.as_weak();
        let apply = apply.clone();
        move |i| {
            apply(&weak, i, &|g| {
                g.user_title = None;
                g.user_art = None;
            })
        }
    });

    app.on_game_cover({
        let weak = app.as_weak();
        let inst = Arc::clone(&install_state);
        let all_items = Arc::clone(&all_items);
        let filter = Arc::clone(&filter);
        let order = Arc::clone(&order);
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            let games = all_items.lock().unwrap();
            let indices = library_view::visible_indices(
                &games,
                &filter.lock().unwrap(),
                *order.lock().unwrap(),
            );
            let Some(&i) = usize::try_from(index).ok().and_then(|i| indices.get(i)) else {
                return;
            };
            let game = &games[i];
            let source = std::path::PathBuf::from(game.artwork())
                .parent()
                .filter(|p| p.is_dir())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(explorer::starting_path);
            let mut e = inst.lock().unwrap();
            e.open(install::Mode::Cover, source);
            e.cover_game = Some(game.path.clone());
            render_install(&app, &e);
        }
    });

    // #595/#642: el sondeo va en su hilo y re-escanea al terminar. Bloquear el
    // arranque por él sería pagar un proceso —uno por core— antes de mostrar
    // nada, y lo que aporta puede llegar un segundo después sin que nadie lo
    // note: hasta entonces el eje del core dice «todavía no se comprobó», que
    // no impide jugar.
    //
    // #642 — se sondean TODOS los cores configurados y no sólo el que gradúa los
    // packs, porque desde #631 el core decide solo si se puede jugar: quien
    // anotó un core por consola necesita que cada uno se compruebe. Uno por
    // ruta, no uno por consola ni uno por juego (`cores::a_sondear`).
    {
        let (runtime, paths, default_core) = {
            let c = cfg.lock().unwrap();
            (c.runtime(), cores::to_probe(&c), c.default_core.clone())
        };
        let cb = Arc::clone(&core_build);
        let weak = app.as_weak();
        let (cfg2, all_items2, filter2, order2) = (
            cfg.clone(),
            all_items.clone(),
            filter.clone(),
            order.clone(),
        );
        std::thread::spawn(move || {
            for path in paths {
                let v = core_probe::validate(&runtime, &path);
                // #595 — el `build_id` con el que se gradúan los packs sale del
                // core POR DEFECTO, que es el que lanza. Nombre + versión: es lo
                // que el core declara de sí mismo, y es lo que un pack exige.
                if path == default_core
                    && let core_probe::Verdict::Valid(i) = &v
                {
                    *cb.lock().unwrap() = Some(format!("{} {}", i.name, i.version));
                }
                // #642 — el DETALLE del rechazo va acá y no a la ficha: nombra
                // rutas, y el log ya redacta (#621). Es lo que después se lee en
                // el paquete de diagnóstico (#622) cuando alguien reporta que su
                // core no le anda.
                if !v.is_usable() {
                    log_warning!("play", "core rechazado: {} — {}", v.title(), v.remedy());
                }
                cores::record(path, v);
            }
            // Y se re-proyecta SIEMPRE, no sólo cuando el sondeo salió bien.
            // Antes de #642 un rechazo no repintaba nada —no había nada que
            // repintar, porque el eje salía del archivo— y ahora sí: la tarjeta
            // pasa de «Compatible» a «El core no sirve» justo acá.
            slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    refresh_ui(&app, &all_items2, &filter2, &order2, &cfg2);
                }
            })
            .ok();
        });
    }

    // El sondeo de cores dispara un único escaneo al terminar.

    // ---- #311: el mando mueve la interfaz -----------------------------------
    //
    // Slint navega con teclado y nadie lee el mando. El puente traduce a
    // ACCIONES —arriba, abajo, aceptar— y acá se conectan a lo mismo que ya
    // hacen las teclas. Las dos rutas terminan en la misma llamada: si se
    // separaran, el mando y el teclado harían cosas distintas y sólo se
    // notaría probando los dos.
    //
    // El destino depende de qué hay abierto: con la pantalla previa arriba
    // manda ella, y si no, la lista. Es la misma regla que un modal impone al
    // teclado, escrita para el mando.
    // #588 — el nombre del mando conectado, para el paso 2 del wizard. Lo
    // escribe el hilo del mando; acá sólo se lee.
    let gamepad_name: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let _pad_stop = {
        let weak = app.as_weak();
        let weak_connection = app.as_weak();
        let weak_button = app.as_weak();
        gamepad::start(
            move |action| {
                let weak = weak.clone();
                // El hilo del mando no puede tocar la UI: se salta al hilo del
                // event loop, que es el único que puede.
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak.upgrade() else { return };
                    // #586 — el ruteo vive en `pad_router`, que es una función
                    // pura y por eso se puede probar sin abrir una ventana ni
                    // conectar un mando. Acá sólo se lee el estado y se aplica el
                    // efecto: las 8 superficies y la regla de «Atrás nunca cierra»
                    // se verifican en sus tests.
                    let e = pad_state(&app);
                    let ef = pad_router::route(&e, action);
                    // «El mando no hace nada» es imposible de diagnosticar a ojo:
                    // hay que saber qué superficie tenía el foco. Con
                    // AYTHER_PLAY_DEBUG_PAD=1 queda escrito (EP12, observabilidad).
                    if std::env::var_os("AYTHER_PLAY_DEBUG_PAD").is_some() {
                        eprintln!("[pad] {} · {action:?} → {ef:?}", pad_router::hints(&e));
                    }
                    apply_effect(&app, ef);
                });
            },
            Arc::clone(&button_capture),
            move |button| {
                // Un botón crudo para el diálogo de controles. Llega como dato
                // (`"south"`, `"leftshoulder"`…), no como gesto: la asignación la
                // decide `controls::Dialogo` en el hilo de la UI.
                let weak = weak_button.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.invoke_controls_button(button.into());
                    }
                });
            },
            move |has_gamepad| {
                // El pie dibuja las píldoras del mando o las teclas del teclado
                // según esto, y cambia EN CALIENTE: enchufar el mando con Play
                // abierto tiene que verse en el pie sin reiniciar (spec §14).
                let weak = weak_connection.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Gamepad>().set_connected(has_gamepad);
                    }
                });
            },
            Arc::clone(&gamepad_name),
        )
    };

    // El pie de botones es BOTONERA, no leyenda: un click en «B Cerrar» entra
    // por el MISMO router que el botón B del mando. Es la mitad mouse de los
    // diálogos — su velo deja el pie a la vista justamente para esto.
    app.on_hint_pressed({
        let weak = app.as_weak();
        move |key| {
            let Some(app) = weak.upgrade() else { return };
            let Some(a) = gamepad::hint_action(&key) else {
                return;
            };
            let e = pad_state(&app);
            apply_effect(&app, pad_router::route(&e, a));
        }
    });

    // ---- #311: configuración previa al lanzamiento --------------------------
    //
    // `launch` ya no lanza: abre la pantalla. Lanzar directo y dejar la
    // configuración «para después» significaría que la primera partida siempre
    // se juega con algo que el jugador no eligió — y en un pack con varios
    // perfiles esa primera impresión es la que decide si vuelve a probar.
    //
    // El estado vive en un Arc compartido y no en la UI: la UI muestra filas ya
    // formateadas, y quien decide qué significa cada fila es el modelo, que
    // tiene tests.
    let pending_launch: Arc<Mutex<Option<PendingLaunch>>> = Arc::new(Mutex::new(None));

    // #313: el formulario en curso. Vive acá —y no dentro del callback— porque
    // lo llenan tres callbacks distintos (puntuar, guardar, posponer) y lo
    // arma un cuarto (el fin de la sesión, en otro hilo).
    let feedback_cur: Arc<Mutex<feedback::Feedback>> =
        Arc::new(Mutex::new(feedback::Feedback::default()));

    app.on_launch({
        let weak = app.as_weak();
        move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let model = app.get_games();
            let Some(entry) = model.row_data(idx as usize) else {
                return;
            };
            app.invoke_launch_entry(entry);
        }
    });

    // #594 — lanzar por ENTRADA, no por índice.
    //
    // Inicio tiene sus propios modelos —el hero y la fila de recientes— y un
    // índice sólo significa algo dentro de un modelo. Pasar la entrada saca el
    // problema de raíz: trae su ruta, así que no importa de qué lista salió.
    // `launch(i)` quedó como el atajo de la biblioteca, y ahora delega acá.
    app.on_launch_entry({
        let weak = app.as_weak();
        let pending_launch = pending_launch.clone();
        let cfg = cfg.clone();
        let lib = lib.clone();
        let core_build = core_build.clone();
        let force_configuration = force_configuration.clone();
        move |entry| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_playing() || PREPARING.swap(true, Ordering::AcqRel) { return; }
            PREPARE_CANCEL.store(false, Ordering::Release);
            let configuration = cfg.lock().unwrap().clone();
            let rom = std::path::PathBuf::from(entry.path.as_str());
            let pack = entry.has_pack.then(|| std::path::PathBuf::from(entry.pack_path.as_str())).filter(|p| !p.as_os_str().is_empty());
            let name = entry.name.to_string();
            let platform = entry.platform.to_string();

            let clean = app.get_clean_run();
            let forced = force_configuration.swap(false, Ordering::Relaxed);
            let (weak, pending_launch, lib, build) = (weak.clone(), pending_launch.clone(), lib.clone(), core_build.lock().unwrap().clone());
            app.set_status(tr!("Preparando juego…").into());
            app.set_scanning(true);
            app.set_scan_stage(tr!("Preparando juego…").into());
            app.set_scan_done(0);
            app.set_scan_total(0);
            std::thread::spawn(move || {
                let prepared = (|| -> Result<_, String> {
                    if cores::state(&configuration, &platform).blocks_launch() { return Err("No se puede iniciar este juego con el core configurado.".into()); }
                    let (rom_guard, prepared_game) = library::prepare_rom(&lib, &rom, &PREPARE_CANCEL)?;
                    let crc = prepared_game.crc32;
                    let file = launch_config::config_path(&config::Config::config_dir(), &rom, pack.as_deref());
                    let already_configured = file.is_file();
                    let mut lc = launch_config::load(&file);
                    let pack = if !lc.is_hd() && !forced { None } else { pack };
                    let mut reservations = vec![Arc::new(rom_guard)];
                    let details = if let Some(path) = &pack {
                        let guard = file_revision::reserve(path).map_err(|e|format!("El pack seleccionado no está disponible o está en uso: {e}. Elegí otro pack o jugá al original."))?;
                        reservations.push(Arc::new(guard));
                        let repo = pack_cache::global().ok_or("La caché de packs todavía no está disponible")?;
                        let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
                        let snapshot = repo.inspect(path, &PREPARE_CANCEL, &mut |done,total| {
                            if last.elapsed() >= std::time::Duration::from_millis(100) || done == total {
                                scan_progress(weak.clone(), tr!("Comprobando pack…"), (done/1024) as usize, (total/1024) as usize);
                                last = std::time::Instant::now();
                            }
                        })?;
                        let context = ayther_core::pack_validate::SessionCtx { rom_crc32: u32::from_str_radix(&crc,16).ok(), platform: library::platform_id(&platform), core_build_id: build.as_deref(), ..Default::default() };
                        let details = pack_details::from_snapshot(&snapshot, &context);
                        if !details.playable() { return Err(format!("No se puede usar el pack seleccionado: {}. Elegí otro pack o jugá al original.",details.compat_label())); }
                        details
                    } else { pack_details::PackDetails::default() };
                    let caps = pack.as_deref().map(pack_meta::capabilities).unwrap_or_default();
                    lc.clamp_to(&caps);
                    let resume_from = if clean { None } else {
                        let gid = rom.file_stem().and_then(|s|s.to_str()).unwrap_or("unknown");
                        let profile = if lc.profile.is_empty() { saves::DEFAULT_PROFILE } else { lc.profile.as_str() };
                        saves::last(&configuration.saves_dir, gid, profile).filter(|v|v.is_compatible_with(&crc)).map(|v|v.path)
                    };
                    if PREPARE_CANCEL.load(Ordering::Acquire) { return Err("Preparación cancelada".into()); }
                    let open_screen = launch_config::open_prelaunch(already_configured, forced);
                    Ok((PendingLaunch { rom, crc32: crc, pack, caps, cfg: lc, file, name, resume_from, remember_configuration: open_screen, reservations }, details, open_screen))
                })();
                let _ = slint::invoke_from_event_loop(move || {
                    PREPARING.store(false, Ordering::Release);
                    let Some(app) = weak.upgrade() else { return };
                    app.set_scanning(false);
                    match prepared {
                        Err(error) => app.set_status(error.into()),
                        Ok((pending, details, open_screen)) => {
                            if PREPARE_CANCEL.load(Ordering::Acquire) {
                                app.set_status("Preparación cancelada".into());
                                return;
                            }
                            app.set_config_game(pending.name.as_str().into());
                            app.set_config_pack(details.name.as_str().into());
                            app.set_config_row(0);
                            app.set_config_rows(build_config_model(&pending.caps, &pending.cfg));
                            app.set_credit_lines(build_credit_model(pending.pack.as_deref()));
                            app.set_credits_open(false);
                            app.set_detail_rows(build_detail_model(&details));
                            app.set_detail_pack(details.name.as_str().into());
                            app.set_detail_compat(details.compat_label().into());
                            app.set_detail_playable(details.playable());
                            app.set_detail_open(false);
                            app.set_config_open(open_screen);
                            *pending_launch.lock().unwrap() = Some(pending);
                            app.set_clean_run(false);
                            if !open_screen { app.invoke_config_accept(); }
                        }
                    }
                });
            });
        }
    });
    app.on_config_adjust({
        let weak = app.as_weak();
        let pending_launch = Arc::clone(&pending_launch);
        move |row, delta| {
            let Some(app) = weak.upgrade() else { return };
            let mut guard = pending_launch.lock().unwrap();
            let Some(p) = guard.as_mut() else { return };
            let rows = launch_config::config_rows(&p.caps, &p.cfg);
            let Some(r) = rows.get(row as usize) else {
                return;
            };
            launch_config::adjust(&p.caps, &mut p.cfg, &r.kind, delta);
            // Las filas se REGENERAN: los valores de unas dependen de otras
            // (cambiar de perfil descarta los ajustes sueltos), y actualizar
            // sólo la tocada dejaría el resto mintiendo.
            app.set_config_rows(build_config_model(&p.caps, &p.cfg));
        }
    });

    // #627 — «Valores predeterminados», el botón del pie. Es la misma acción
    // que la fila que estaba al final de la lista (`RowKind::Restore`), movida a
    // donde se decide: al lado de «Jugar».
    app.on_config_restore({
        let weak = app.as_weak();
        let pending_launch = Arc::clone(&pending_launch);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut guard = pending_launch.lock().unwrap();
            let Some(p) = guard.as_mut() else { return };
            launch_config::adjust(&p.caps, &mut p.cfg, &launch_config::RowKind::Restore, 1);
            app.set_config_rows(build_config_model(&p.caps, &p.cfg));
        }
    });

    app.on_config_cancel({
        let weak = app.as_weak();
        let pending_launch = Arc::clone(&pending_launch);
        move || {
            if let Some(app) = weak.upgrade() {
                app.set_config_open(false);
            }
            // Cancelar NO guarda: el jugador se fue sin decidir, y persistir lo
            // que estuvo mirando convertiría un vistazo en una preferencia.
            *pending_launch.lock().unwrap() = None;
        }
    });

    app.on_config_accept({
        let weak = app.as_weak();
        let cfg  = Arc::clone(&cfg);
        let pending_launch = Arc::clone(&pending_launch);
        let feedback_cur = Arc::clone(&feedback_cur);
        // #594: el closure se queda con SU copia de estos cuatro — anotar la
        // sesión al terminar necesita la biblioteca y la vista, y el `move`
        // se llevaría los originales si se clonaran adentro.
        let lib    = Arc::clone(&lib);
        let all_items  = Arc::clone(&all_items);
        let filter = Arc::clone(&filter);
        let order  = Arc::clone(&order);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(mut p) = pending_launch.lock().unwrap().take() else {
                return;
            };
            if p.reservations.is_empty() {
                PREPARING.store(true, Ordering::Release);
                PREPARE_CANCEL.store(false, Ordering::Release);
                let (weak, lib, pending) = (weak.clone(), lib.clone(), pending_launch.clone());
                std::thread::spawn(move || {
                    let result = library::prepare_rom(&lib, &p.rom, &PREPARE_CANCEL);
                    let _ = slint::invoke_from_event_loop(move || {
                        PREPARING.store(false, Ordering::Release);
                        let Some(app) = weak.upgrade() else { return };
                        match result {
                            Ok((file, game)) if !PREPARE_CANCEL.load(Ordering::Acquire) => {
                                p.reservations.push(Arc::new(file));
                                p.crc32 = game.crc32;
                                *pending.lock().unwrap() = Some(p);
                                app.invoke_config_accept();
                            }
                            Ok(_) => app.set_status("Preparación cancelada".into()),
                            Err(error) => app.set_status(error.into()),
                        }
                    });
                });
                return;
            }
            if !p.cfg.is_hd() {
                p.pack = None;
                p.reservations.truncate(1);
            }
            app.set_config_open(false);
            // Se guarda al ACEPTAR y no en cada movimiento: lo que se persiste
            // es con qué se jugó, no por dónde pasó el cursor. El modo seguro
            // (#601) es la excepción: es un intento de rescate, no una
            // preferencia, y recordarlo dejaría al juego apagado para siempre.
            if p.remember_configuration { launch_config::save(&p.file, &p.cfg); }

            let game_id = p.rom.file_stem().and_then(|s| s.to_str())
                              .unwrap_or("unknown").to_string();
            let weak2   = weak.clone();
            let runtime = cfg.lock().unwrap().runtime();
            let core    = cfg.lock().unwrap().default_core.clone();
            let name    = p.name.clone();
            app.set_playing(true);
            app.set_status(tr!("Jugando: {}", name).into());

            // #313: lo que la pregunta necesita saber del pack, capturado ANTES
            // de lanzar (el callback corre en otro hilo y la fila ya no está).
            let fb_pack_id = p.pack.as_deref()
                .and_then(|q| q.file_stem()).map(|q| q.to_string_lossy().into_owned())
                .unwrap_or_default();
            // #602 — el pack se lee UNA vez y de ahí salen las dos cosas. Leerlo
            // dos veces —una para el nombre y otra para la versión— abriría un
            // `.ay` de cien megas dos veces por sesión, y para el mismo dato.
            let fb_info = p.pack.as_deref().and_then(|q| pack_meta::read(q).1);
            let fb_pack_name = fb_info.as_ref().map(|i| i.name.clone())
                .unwrap_or_else(|| fb_pack_id.clone());
            let fb_pack_ver = fb_info.as_ref().map(|i| i.version.clone())
                .unwrap_or_default();
            let fb_game_id = game_id.clone();
            let fb_cur     = Arc::clone(&feedback_cur);
            let fb_cfg     = Arc::clone(&cfg);
            // #594: la ruta de la ROM, capturada ANTES de lanzar. El callback
            // corre en otro hilo y con la fila ya fuera de alcance; es la clave
            // con la que la biblioteca anota la sesión.
            let session_rom    = p.rom.to_string_lossy().into_owned();
            // #602 — dónde mirar al terminar, y desde cuándo. El corte se toma
            // ANTES de lanzar: lo que ya estaba no es de esta partida.
            let session_saves  = cfg.lock().unwrap().saves_dir.clone();
            let session_captures_dir = session::captures_dir(&config::Config::config_dir());
            let session_started_at = std::time::SystemTime::now();
            // #603 — los guardados del layout plano se mudan ANTES de jugar.
            //
            // Acá y no al arrancar Play: migrar la biblioteca entera en el
            // arranque tocaría cientos de archivos por un juego que quizá nadie
            // abra, y la única vez que importa que estén en su lugar es cuando
            // se va a jugar ESE juego. Mueve y no copia: dejar el viejo haría
            // que la próxima versión lo migrara otra vez, y la partida que el
            // jugador siguió jugando aparecería duplicada y desactualizada.
            {
                let moved_count = saves::migrate_legacy(
                    &session_saves, &game_id, &date::compact_timestamp());
                if moved_count > 0 {
                    eprintln!("[play] {moved_count} guardado(s) de {game_id}                                mudados al layout con perfil");
                }
            }
            let session_lib    = Arc::clone(&lib);
            let (session_games, session_filter, session_order, session_cfg) =
                (Arc::clone(&all_items), Arc::clone(&filter), Arc::clone(&order), Arc::clone(&cfg));

            // #601 - el LOCK, antes que nada.
            //
            // Dos instancias del mismo juego escriben en los mismos guardados y
            // la última en cerrar gana: lo que se pierde es la partida de la
            // primera, sin aviso. El lock lo impide y el aviso dice quién lo
            // tiene, para que «no arranca» no se lea como que Play se colgó.
            //
            // Un lock del propio proceso no bloquea (`session::tomar`): sería
            // Play impidiéndose a sí mismo relanzar un juego que ya cerró.
            let data_dir = cfg.lock().unwrap().sessions_dir();
            let session_id = launch_manifest::new_id();
            let lock = match session::take_focus(&data_dir, &p.rom, &p.name, &session_id,
                                           std::process::id(), &session::is_process_alive) {
                session::GameLockResult::Acquired(l) => l,
                session::GameLockResult::AlreadyRunning(d) => {
                    app.set_toast_text(
                        tr!("{} ya está en juego (pid {})", d.game, d.pid).as_str().into());
                    return;
                }
            };
            app.set_toast_text("".into());

            // #596 — el manifest de la sesión se escribe ANTES de lanzar.
            //
            // Antes, si no se escribiera, quedaría una partida sin registro: no
            // se podría reproducir ni detectar como huérfana si el proceso
            // muere. Que falle escribirlo NO impide jugar —el jugador apretó A,
            // no pidió un archivo— pero se avisa: una sesión sin manifest es una
            // que el modo seguro (#601) no va a poder investigar.
            // #599 — el perfil de salida GLOBAL entra ACÁ, al lanzar.
            //
            // Acá y no en la config guardada del juego: escribirlo allá lo
            // convertiría en una elección por juego, y el próximo cambio en
            // Ajustes ya no alcanzaría a ese juego. Y acá y no en `spawn`, para
            // que el manifest —que es el registro de lo que se lanzó (#596)—
            // diga exactamente lo mismo que la línea de comandos.
            //
            // Era el defecto: Ajustes guardaba el perfil global y nadie lo leía,
            // así que elegirlo ahí no cambiaba nada al jugar.
            let effective_launch_config = {
                let global = cfg.lock().unwrap().output_global.clone();
                launch_config::with_global_output(&p.cfg, &global)
            };
            let mut manifest = launch_manifest::LaunchManifest::new(
                &session_id,
                &p.rom,
                &p.crc32,
                &core,
                p.pack.as_deref(),
                &effective_launch_config,
            );
            // #604: el manifest anota DESDE DÓNDE se reanudó. Es lo que hace
            // que copiarlo reproduzca la misma partida y no el mismo juego
            // desde el principio.
            manifest.save = p.resume_from.as_ref()
                .map(|g| g.to_string_lossy().into_owned()).unwrap_or_default();
            let manifest_path = match launch_manifest::write(&data_dir, &manifest) {
                Ok(r) => {
                    // La ruta del manifest es DIAGNÓSTICO: la necesita el modo
                    // seguro (#601) y quien lea el log, no el jugador que está
                    // esperando que arranque su juego. Por eso va acá y ya no a
                    // la pantalla.
                    log_verbose!("play", "manifest de la sesión: {}", r.display());
                    Some(r)
                }
                Err(e) => {
                    eprintln!("[play] no se pudo escribir el manifest de la sesión: {e}");
                    None
                }
            };
            // #601: lo que el cierre y el modo seguro van a necesitar cuando
            // esto termine mal. Se captura ACÁ, con la fila todavía a la vista:
            // el callback corre en otro hilo y para entonces ya no está.
            let session_dir  = launch_manifest::session_dir(&data_dir, &session_id);
            let session_man  = manifest.clone();
            // La MISMA carpeta que la del hook de pánico: dos definiciones de
            // «dónde van los diagnósticos» es cómo la mitad de la evidencia
            // termina en un directorio que nadie mira.
            let session_diag = cfg.lock().unwrap().diagnostics_dir();
            let session_pending_launch = Arc::clone(&pending_launch);
            let session_caps = p.caps.clone();
            let session_file = p.file.clone();
            let session_crc  = p.crc32.clone();
            let session_name = p.name.clone();

            // #600 - el overlay se abre ACA, con el manifest ya escrito y el
            // proceso todavia sin nacer, y se cierra con el evento `ready`.
            // Cerrarlo por temporizador acertaria en una maquina y mentiria en
            // las otras dos.
            //
            // Los pasos dicen lo que REALMENTE paso, incluso cuando salio mal:
            // lo que no se pudo hacer se anuncia en vez de mostrarse en verde.
            //
            // Y lo dicen EN CASTELLANO DE JUGADOR (#627). Decian «Launch
            // manifest escrito · C:\Users\…\sessions\20260828T203234Z-cc54\
            // launch.toml», «Lanzando el Runtime…» y «Esperando AYTHER_STATUS
            // ready…»: tres renglones que nombran piezas internas y una ruta de
            // ochenta caracteres, en la pantalla que mira alguien que sólo
            // quiere jugar. Lo tecnico —la ruta, el pid, el evento— vive en el
            // log, que es donde sirve.
            app.set_session_game(p.name.as_str().into());
            app.set_session_warning("".into());
            app.set_session_manifest(match manifest_path.as_deref() {
                Some(_) => tr!("Partida preparada"),
                // El que sale mal SÍ tiene que ser preciso: es lo que el jugador
                // va a contar cuando pida ayuda.
                None    => tr!("No se pudo preparar la partida"),
            }.as_str().into());
            app.set_session_process(tr!("Abriendo el juego…").as_str().into());
            app.set_session_wait(tr!("Esperando a que arranque…").as_str().into());
            app.set_session_step(1);
            app.set_session_preparing(true);
            let weak_ev = weak.clone();

            // Los controles del jugador, FRESCOS de la config: el TOML se
            // escribe en cada lanzamiento y no cuando se edita, así no puede
            // quedar rancio. Sin cambios no se escribe ni viaja: el runtime
            // ya sabe su fábrica solo.
            let input_map = {
                let c = cfg.lock().unwrap();
                let ctl = controls::Controls::of(
                    &c.control_keys, &c.control_buttons);
                ctl.has_changes().then(|| {
                    let path = config::Config::config_dir().join("controls.toml");
                    match ctl.write(&path) {
                        Ok(()) => Some(path),
                        Err(e) => {
                            // Sin el mapa se juega igual, con la fábrica: se
                            // avisa y no se frena — quien apretó Jugar quiere
                            // jugar.
                            log_error!("play", "no se pudo escribir controls.toml: {e}");
                            None
                        }
                    }
                }).flatten()
            };

            let trust_registry = cfg.lock().unwrap().pack_trust_registry(p.pack.as_deref()).map(std::path::Path::to_path_buf);
            spawn::launch(spawn::LaunchRequest {
                          reservations: &p.reservations,
                          runtime: &runtime, core: &core, rom: &p.rom,
                          pack: p.pack.as_deref(), game_name: name,
                          trust_registry: trust_registry.as_deref(),
                          cfg: Some(&effective_launch_config), manifest: manifest_path.as_deref(),
                          saves_dir: &session_saves.clone(), rom_crc32: &session_crc.clone(),
                          input_map: input_map.as_deref(), save: p.resume_from.as_deref(),
                          },
                          move |e| {
                // El evento llega en el hilo del drenaje; la UI se toca solo
                // desde el loop de Slint.
                let (weak_ev, e) = (weak_ev.clone(), e.clone());
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak_ev.upgrade() else { return };
                    use runtime_event::RuntimeEvent as Ev;
                    match e {
                        // `ready` cierra el overlay: el juego ya esta en
                        // pantalla y dejarlo puesto taparia la partida.
                        Ev::Ready { .. } => {
                            app.set_session_step(3);
                            app.set_session_preparing(false);
                        }
                        Ev::NowPlaying { title, game_id } =>
                            app.set_session_game(
                                if title.is_empty() { game_id } else { title }
                                .as_str().into()),
                        // Un aviso se MUESTRA y no corta. Es la unica
                        // oportunidad de decir «esto arranca sin musica» antes
                        // de que el jugador lo note oyendo.
                        warning @ Ev::Warning { .. } => {
                            let text = runtime_event::audio_diagnostic_message(
                                &warning,
                                &i18n::lang(),
                            )
                            .or_else(|| runtime_event::music_warning_message(
                                &warning,
                                &i18n::lang(),
                            ))
                            .or_else(|| match &warning {
                                Ev::Warning { reason, .. } => Some(reason.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                            app.set_session_warning(text.as_str().into());
                        }
                        // Un error SI cierra el overlay: si la sesion no
                        // arranco, «Preparando» se quedaria puesto para siempre.
                        Ev::Error { reason, .. } => {
                            app.set_session_preparing(false);
                            app.set_status(tr!("No se pudo jugar: {}", reason).into());
                        }
                        Ev::Exit { .. } | Ev::CrashTest => app.set_session_preparing(false),
                    }
                });
            },
                          move |result| {
                let failure_notice = result.failure.notice();
                let status = if result.clean_exit {
                    format!("Last played: {}", result.game_name)
                } else {
                    failure_notice.title.clone()
                };
                // #313: ¿se pregunta? La decisión es de `feedback.rs` y se
                // toma ACÁ, con el runtime ya cerrado — por eso «no aparece
                // durante la partida» no es una condición que haya que
                // chequear, es dónde está el código.
                let dir   = fb_cfg.lock().unwrap().feedback_dir.clone();
                let st    = feedback::FeedbackState::load(&dir);
                // #602 — LA VERSIÓN REAL DEL PACK. Es el CA de la issue.
                //
                // Hasta acá iba `""`, así que la clave de la opinión era sólo
                // el pack: quien opinaba sobre la 1.0 no volvía a ser
                // preguntado nunca, y el autor que arreglaba su pack no tenía
                // cómo saber si había mejorado. Una opinión sobre un pack que
                // cambió es una opinión sobre otra cosa.
                let pack_version = fb_pack_ver.as_str();
                let ask = st.should_ask(&fb_pack_id, pack_version, result.played_secs);
                if ask {
                    *fb_cur.lock().unwrap() = feedback::Feedback::new(
                        &fb_pack_id,
                        pack_version,
                        &fb_game_id,
                        result.played_secs,
                    );
                }
                // #594 — EL CA: `played_secs` deja de morir en la encuesta.
                //
                // Se anota SIEMPRE, aunque la salida no haya sido limpia: un
                // juego que crasheó a los diez minutos se jugó diez minutos, y
                // no anotarlo haría que «Continuar» ofreciera el anterior —
                // justo cuando lo que el jugador quiere es volver al que se le
                // cayó.
                if let Ok(mut l) = session_lib.lock()
                    && let Err(error) = l.record_session_once(&session_man.id, &session_rom, result.played_secs, date::epoch_secs()) {
                        let weak = weak2.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak.upgrade() { app.set_status(format!("La partida terminó, pero no se pudieron guardar las estadísticas: {error}").into()); }
                        });
                }

                // #601 - la sesión se CIERRA en disco, pase lo que pase.
                //
                // Es lo único que distingue «terminó» de «Play murió con el
                // juego abierto», y tiene que sobrevivir a que este proceso
                // deje de existir. Sin el archivo, la próxima vez que Play
                // arranque va a ofrecer recuperar una partida que terminó bien.
                let _ = session::close(&session_dir, &session::SessionExit {
                    ended_at: date::now_utc(),
                    clean_exit:    result.clean_exit,
                    exit_code:    result.exit_code,
                    savestate: result.savestate_path.clone(),
                    elapsed_seconds:  result.played_secs,
                });
                session::release(&lock);

                // #603 — se poda DESPUÉS de escribir y nunca antes: podar
                // primero podría dejar la carpeta sin nada si la escritura que
                // sigue falla. El perfil sale del manifest, que es lo que
                // realmente corrió.
                let pruned_count = saves::prune(&session_saves, &fb_game_id,
                                               &session_man.profile,
                                               saves::MAX_VERSIONS);
                if pruned_count > 0 {
                    eprintln!("[play] {pruned_count} version(es) vieja(s) de guardado                                retiradas");
                }

                // Y si terminó mal, el diagnóstico REDACTADO queda escrito
                // antes de preguntar nada. Es el CA: después de un crash tiene
                // que quedar algo que se pueda leer y mandar, y depender de que
                // el jugador apriete un botón lo perdería justo cuando la
                // ventana se cierra sola.
                let mut crash_txt = String::new();
                if !result.clean_exit {
                    let session_exit = session::read_exit(&session_dir);
                    let mut diag = session::diagnostic(&session_man, session_exit.as_ref());
                    diag.push_str(&result.failure.diagnostic());
                    let _ = std::fs::create_dir_all(&session_diag);
                    let destination = session_diag.join(format!("{}.txt", session_man.id));
                    match std::fs::write(&destination, &diag) {
                        Ok(()) => crash_txt = tr!(
                            "Si vuelve a fallar, el diagnóstico redactado (sin rutas ni nombre de usuario) queda en {}.",
                            destination.display()),
                        Err(e) => {
                            eprintln!("[play] no se pudo escribir el diagnóstico: {e}");
                            crash_txt = tr!("No se pudo escribir el diagnóstico de la sesión.");
                        }
                    }
                    // El modo seguro necesita saber QUÉ había. Se deja armado
                    // acá porque es el único momento en que se tienen las dos
                    // mitades: el manifest de lo que corrió y el resultado.
                    *session_pending_launch.lock().unwrap() = Some(PendingLaunch {
                        reservations: Vec::new(),
                        rom:  std::path::PathBuf::from(&session_man.rom),
                        crc32: session_crc.clone(),
                        pack: None,
                        caps: session_caps.clone(),
                        cfg:  session::safe_mode(&session_man, "previo").config(),
                        file: session_file.clone(),
                        name: session_name.clone(),
                        // El modo seguro NO reanuda: si el guardado es lo que
                        // colgaba el juego, cargarlo repetiría el crash — y el
                        // modo seguro existe para arrancar sin lo que pudo
                        // haberlo causado.
                        resume_from: None,
                        remember_configuration: false,
                    });
                }
                let crashed = !result.clean_exit;

                // #602 — el balance de la sesión: lo que quedó en el disco.
                //
                // Se cuenta acá, con el Runtime ya cerrado, y no durante la
                // partida: Play no ve nada mientras se juega, y un contador que
                // tendría que mantener sincronizado sería un segundo estado
                // para el mismo hecho — el que se desincroniza es el que se
                // muestra.
                let session_summary = session::summarize_session(
                    &session_saves, &session_captures_dir, &fb_game_id, &session_man.profile,
                    result.played_secs, session_started_at);
                let summary_time = if session_summary.elapsed_seconds < 60 {
                    tr!("{} s", session_summary.elapsed_seconds)
                } else {
                    tr!("{} min", session_summary.elapsed_seconds / 60)
                };
                let summary_saves = match (session_summary.sram, session_summary.savestates) {
                    (false, 0) => tr!("ninguno"),
                    (true,  0) => tr!("SRAM"),
                    (false, 1) => tr!("1 estado"),
                    (false, n) => tr!("{} estados", n),
                    (true,  1) => tr!("SRAM + 1 estado"),
                    (true,  n) => tr!("SRAM + {} estados", n),
                };
                let summary_captures = session_summary.captures.to_string();
                let summary_has_activity = session_summary.has_activity();

                let played_min = result.played_secs / 60;
                let pack_label = fb_pack_name.clone();

                let weak3 = weak2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak3.upgrade() {
                        app.set_playing(false);
                        app.set_status(status.into());
                        app.set_session_preparing(false);
                        // #601: el crash abre SU diálogo. La encuesta de
                        // opinión queda para otra vez: preguntarle a alguien
                        // qué le pareció el pack justo después de que se le
                        // cayó el juego es preguntar por el crash.
                        if crashed {
                            app.set_crash_title(failure_notice.title.into());
                            app.set_crash_explanation(failure_notice.explanation.into());
                            app.set_crash_pack_rejected(failure_notice.pack_rejected);
                            app.set_crash_detail(crash_txt.as_str().into());
                            app.set_crash_open(true);
                        }
                        // La sesión cambió `ultima_vez`, y de eso dependen el
                        // hero de Inicio y el orden por recientes: sin
                        // re-proyectar, el juego del que se acaba de salir no
                        // aparece en «Continuar» hasta el próximo escaneo.
                        if let Ok(mut t) = session_games.lock()
                            && let Ok(l) = session_lib.lock() { *t = l.get_all(); }
                        refresh_ui(&app, &session_games, &session_filter, &session_order, &session_cfg);
                        if ask && !crashed {
                            let rows: Vec<FeedbackRowData> = feedback::DIMENSIONS
                                .iter()
                                .map(|d| FeedbackRowData { label: (*d).into(), value: 0 })
                                .collect();
                            app.set_feedback_rows(ModelRc::new(VecModel::from(rows)));
                            app.set_feedback_row(0);
                            app.set_feedback_pack(pack_label.into());
                            app.set_feedback_played(
                                tr!("{} min jugados", played_min).as_str().into());
                            // #602 — las tres baldosas de «04b · Sesión
                            // terminada». Se muestran sólo si hubo algo además
                            // del tiempo: tres ceros son ruido, y se juega sin
                            // guardar todo el tiempo.
                            app.set_summary_time(summary_time.as_str().into());
                            app.set_summary_saves(summary_saves.as_str().into());
                            app.set_summary_captures(summary_captures.as_str().into());
                            app.set_summary_has_activity(summary_has_activity);
                            app.set_feedback_open(true);
                        }
                    }
                });
            });

            // El proceso ya existe: `launch` lo lanza antes de volver. El paso
            // avanza ACA y no adentro del hilo para que el overlay no muestre
            // «lanzando» cuando el Runtime ya esta corriendo.
            app.set_session_step(2);
        }
    });

    // ---- #313: la opinión, al volver del juego -------------------------------
    //
    // Los callbacks viven acá y el estado en `feedback.rs`. La pantalla se abre
    // desde `on_exit` de la sesión, así que «no aparece durante la partida» no
    // es una condición que haya que chequear: es dónde está el código.
    app.on_feedback_rate({
        let weak = app.as_weak();
        let cur = Arc::clone(&feedback_cur);
        move |row, delta| {
            let Some(app) = weak.upgrade() else { return };
            let rows = app.get_feedback_rows();
            let Some(mut r) = rows.row_data(row as usize) else {
                return;
            };
            // 0 (sin contestar) → 1..5 → vuelve a 0: se puede DESCONTESTAR.
            // Sin eso, tocar una fila sin querer deja una opinión que el
            // jugador no dio y no puede sacar.
            let mut v = r.value + delta;
            if v > 5 {
                v = 0;
            }
            if v < 0 {
                v = 5;
            }
            r.value = v;
            rows.set_row_data(row as usize, r);
            let mut fb = cur.lock().unwrap();
            if (row as usize) < fb.ratings.len() {
                fb.ratings[row as usize] = v.clamp(0, 5) as u8;
            }
        }
    });

    app.on_feedback_send({
        let weak = app.as_weak();
        let cur = Arc::clone(&feedback_cur);
        let cfg = Arc::clone(&cfg);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let fb = cur.lock().unwrap().clone();
            let dir = cfg.lock().unwrap().feedback_dir.clone();
            let mut st = feedback::FeedbackState::load(&dir);
            // Un formulario en blanco se trata como «ahora no»: guardar un
            // archivo vacío sería contarlo después como un voto.
            if fb.answered() {
                match feedback::save(&dir, &fb) {
                    Ok(p) => log_info!("play", "opinión guardada en {}", p.display()),
                    Err(e) => eprintln!("[play] No se pudo guardar la opinión: {e}"),
                }
                st.mark_answered(&fb.pack_id, &fb.pack_version);
            } else {
                st.postpone(&fb.pack_id, &fb.pack_version);
            }
            let _ = st.save(&dir);
            app.set_feedback_open(false);
        }
    });

    app.on_feedback_later({
        let weak = app.as_weak();
        let cur = Arc::clone(&feedback_cur);
        let cfg = Arc::clone(&cfg);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let fb = cur.lock().unwrap().clone();
            let dir = cfg.lock().unwrap().feedback_dir.clone();
            let mut st = feedback::FeedbackState::load(&dir);
            st.postpone(&fb.pack_id, &fb.pack_version);
            let _ = st.save(&dir);
            app.set_feedback_open(false);
        }
    });

    app.on_feedback_never({
        let weak = app.as_weak();
        let cfg = Arc::clone(&cfg);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let dir = cfg.lock().unwrap().feedback_dir.clone();
            let mut st = feedback::FeedbackState::load(&dir);
            st.never_ask = true; // permanente y para TODOS los packs
            let _ = st.save(&dir);
            app.set_feedback_open(false);
            eprintln!("[play] No se volverá a pedir opinión.");
        }
    });

    // ---- on_rescan ----------------------------------------------------------
    app.on_rescan({
        let weak = app.as_weak();
        let cfg = Arc::clone(&cfg);
        let lib = Arc::clone(&lib);
        let dat = dat.clone();
        let cancel = Arc::clone(&scan_cancel);
        let (all_items, filter, order, core_build) = (
            Arc::clone(&all_items),
            Arc::clone(&filter),
            Arc::clone(&order),
            Arc::clone(&core_build),
        );
        move || {
            do_scan(
                weak.clone(),
                Arc::clone(&cfg),
                Arc::clone(&lib),
                dat.clone(),
                Arc::clone(&cancel),
                Arc::clone(&all_items),
                Arc::clone(&filter),
                Arc::clone(&order),
                Arc::clone(&core_build),
            )
        }
    });

    // ---- on_scan_cancel (#591) ----------------------------------------------
    // Sólo levanta la bandera: el hilo que escanea la ve entre archivo y archivo
    // y termina ordenado, guardando lo que alcanzó a mirar. Matar el hilo desde
    // acá dejaría la caché a medio escribir, que es lo contrario del CA.
    app.on_scan_cancel({
        let cancel = Arc::clone(&scan_cancel);
        move || {
            PREPARE_CANCEL.store(true, Ordering::Release);
            SCAN_PENDING.store(false, Ordering::Relaxed);
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });

    // -----------------------------------------------------------------------
    // #588 — el wizard de primer arranque.
    //
    // Se abre solo cuando hace falta: la primera vez, o cuando la configuracion
    // dejo de servir (el core se movio, los terminos cambiaron). En ese caso
    // abre EN EL PASO QUE FALTA, no en el primero: rehacer los seis pasos para
    // arreglar uno es castigar al jugador por haber movido un archivo.
    //
    // `--onboarding` lo fuerza, para poder volver a correrlo mientras Ajustes
    // (EP04.7) no exista.
    // -----------------------------------------------------------------------
    {
        let forced = std::env::args().any(|a| a == "--onboarding");
        let required_step = onboarding::required_step(&cfg.lock().unwrap());
        if forced || required_step.is_some() {
            {
                let mut e = onboarding_state.lock().unwrap();
                e.step = if forced {
                    onboarding::Step::Welcome
                } else {
                    required_step.unwrap()
                };
                // `--onboarding` sobre una instalación ya configurada NO es un
                // primer arranque: hay biblioteca detrás y «B» no puede cerrar
                // Play. `Estado::desde` ya lo dedujo de la configuración; esto
                // sólo evita que el forzado lo contradiga.
                e.first_run = e.first_run && !forced;
            }
            app.set_onboarding_open(true);
            app.set_onboarding_terms_version(onboarding::TERMS_VERSION.into());
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
        }
    }

    app.on_onboarding_move({
        let weak = app.as_weak();
        move |d| {
            let Some(app) = weak.upgrade() else { return };
            let n = if app.get_onboarding_browsing() {
                app.get_onboarding_browse_rows().row_count() as i32
            } else {
                app.get_onboarding_rows().row_count() as i32
            };
            if n > 0 {
                app.set_onboarding_row((app.get_onboarding_row() + d).clamp(0, n - 1));
            }
        }
    });

    // «A». Que hace lo decide `onboarding.rs`, que es donde tambien se escribe
    // el pie que lo anuncia — aca solo se le pregunta. #627: cuando A significa
    // CONTINUAR, esto delega en `onboarding-next`, que es el unico que sabe como
    // termina el wizard (guardar la configuracion y salir a escanear).
    app.on_onboarding_activate({
        let weak = app.as_weak();
        let onboarding_state = Arc::clone(&onboarding_state);
        let cfg = Arc::clone(&cfg);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let row = app.get_onboarding_row().max(0) as usize;
            // Si la lista que se esta mirando cambio —otro paso, el explorador
            // que se abrio o se cerro, otra carpeta— el foco vuelve arriba. Sin
            // esto, abrir el explorador parado en la tercera carpeta dejaba el
            // resaltado en la fila 3 de una lista que el jugador nunca vio, o
            // en ninguna si la carpeta nueva tenia menos entradas.
            let (advance, list_changed) = {
                let mut e = onboarding_state.lock().unwrap();
                let before = (e.step, e.browsing, e.explorer_dir.clone());
                let c = cfg.lock().unwrap().clone();
                let advance = e.activate(&c, row);
                (
                    advance,
                    before != (e.step, e.browsing, e.explorer_dir.clone()),
                )
            };
            if advance {
                app.invoke_onboarding_next();
                return;
            }
            if list_changed {
                app.set_onboarding_row(0);
            }
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
        }
    });

    // Izquierda/derecha: la otra opcion de la fila. Hoy solo el idioma.
    app.on_onboarding_secondary({
        let weak = app.as_weak();
        let onboarding_state = Arc::clone(&onboarding_state);
        move |d| {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut e = onboarding_state.lock().unwrap();
                if !e.browsing && e.step == onboarding::Step::Welcome {
                    e.cycle_language(d);
                }
            }
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
        }
    });

    // El boton secundario del paso (Y en el mando). Su etiqueta sale del mismo
    // `match` que esta accion, en `onboarding.rs`: es lo que impide que vuelva
    // a ofrecer algo que no hace.
    app.on_onboarding_activate_secondary({
        let weak = app.as_weak();
        let onboarding_state = Arc::clone(&onboarding_state);
        move || {
            let Some(app) = weak.upgrade() else { return };
            let row = app.get_onboarding_row().max(0) as usize;
            onboarding_state.lock().unwrap().secondary_action(row);
            app.set_onboarding_row(0);
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
        }
    });

    // «X»: la accion del paso —agregar otra carpeta, elegir otro core—. Es lo
    // que A hacia antes de #627, mudado a su propio boton porque el pie del
    // disenio nuevo pone «A Continuar» en las seis pantallas.
    app.on_onboarding_step_action({
        let weak = app.as_weak();
        let onboarding_state = Arc::clone(&onboarding_state);
        move || {
            let Some(app) = weak.upgrade() else { return };
            onboarding_state.lock().unwrap().step_action();
            app.set_onboarding_row(0);
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
        }
    });

    app.on_onboarding_back({
        let weak = app.as_weak();
        let onboarding_state = Arc::clone(&onboarding_state);
        move || {
            let Some(app) = weak.upgrade() else { return };
            // #627 — en la PORTADA, «B» sale de Play. Es la unica excepcion a
            // «Atras nunca cierra la aplicacion» (§9) y la trae el disenio: en
            // la primera pantalla del primer arranque no hay a donde volver y
            // no hay nada guardado que perder, y un launcher de televisor del
            // que no se puede salir con el mando es peor que la regla.
            let (should_exit, close) = {
                let mut e = onboarding_state.lock().unwrap();
                // QUÉ hace «B» lo decide `onboarding.rs`, que es donde se puede
                // probar: acá sólo se ejecuta. Cerrar Play es la única cosa que
                // el wizard puede hacer y no se puede deshacer, así que la regla
                // no vive en un callback sin tests.
                let should_exit = e.back_closes_app();
                // #599 — abierto desde Ajustes, «B» vuelve A AJUSTES sin
                // guardar. `retroceder` no mueve el paso en ese modo; cerrarlo
                // es de acá, que es quien sabe que hay una pantalla debajo.
                let editing_existing_setup = e.single_step_mode && !e.browsing;
                e.go_back();
                (should_exit, editing_existing_setup)
            };
            if close {
                app.set_onboarding_open(false);
                return;
            }
            if should_exit {
                let _ = slint::quit_event_loop();
                return;
            }
            app.set_onboarding_row(0);
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
        }
    });

    // Continuar / Empezar. En el ultimo paso GUARDA: este es el primer
    // escritor de play_config.toml que Play tuvo nunca.
    app.on_onboarding_next({
        let weak = app.as_weak();
        let onboarding_state = Arc::clone(&onboarding_state);
        let cfg = Arc::clone(&cfg);
        let lib = Arc::clone(&lib);
        let dat = dat.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let completed = {
                let mut e = onboarding_state.lock().unwrap();
                // Con el explorador abierto, «Start» elige la carpeta actual.
                if e.browsing {
                    e.select_current_folder();
                    false
                // #599 — un solo paso, abierto desde Ajustes: «A» guarda y
                // vuelve. Arrastrar al jugador por los cuatro pasos que faltan
                // para que su carpeta quede escrita sería castigarlo por haber
                // entrado a cambiar una cosa.
                } else if e.single_step_mode {
                    if e.can_advance() {
                        let mut c = cfg.lock().unwrap();
                        e.apply(&mut c);
                        c.save();
                        true
                    } else {
                        // Trabado: `traba()` ya dice qué falta y la pantalla lo
                        // muestra. Cerrar acá guardaría a medias.
                        false
                    }
                } else if e.step == onboarding::Step::Summary && e.can_advance() {
                    let mut c = cfg.lock().unwrap();
                    e.apply(&mut c);
                    c.save();
                    true
                } else {
                    e.advance();
                    false
                }
            };
            app.set_onboarding_row(0);
            refresh_onboarding(&app, &onboarding_state.lock().unwrap());
            if completed {
                app.set_onboarding_open(false);
                // El idioma elegido, aplicado en caliente: Slint re-evalua los
                // `@tr` vivos. Sin esto, el jugador elige ingles en el paso 1 y
                // la biblioteca aparece en espanol.
                i18n::change_language(&cfg.lock().unwrap().language);
                i18n::apply_to_slint();
                // Los ajustes cuentan carpetas, cores y juegos: sin repintarlos,
                // el jugador vuelve de agregar una carpeta a una fila que sigue
                // diciendo «1 carpeta».
                render_settings(&app, &cfg, &all_items);
                // Y a escanear lo que acaba de configurar: terminar el wizard y
                // encontrarse una biblioteca vacia se lee como que no funciono.
                do_scan(
                    app.as_weak(),
                    Arc::clone(&cfg),
                    Arc::clone(&lib),
                    dat.clone(),
                    Arc::clone(&scan_cancel),
                    Arc::clone(&all_items),
                    Arc::clone(&filter),
                    Arc::clone(&order),
                    Arc::clone(&core_build),
                );
            }
        }
    });

    // El mando puede enchufarse MIENTRAS el wizard está abierto —es lo más
    // probable, justo en el paso que lo pide— y el estado tiene que verse sin
    // que el jugador toque nada. Medio segundo: no se percibe la espera y no
    // repinta la pantalla al pedo.
    let gamepad_refresh_timer = slint::Timer::default();
    gamepad_refresh_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(500),
        {
            let weak = app.as_weak();
            let onboarding_state = Arc::clone(&onboarding_state);
            let gamepad_name = Arc::clone(&gamepad_name);
            move || {
                let Some(app) = weak.upgrade() else { return };
                if !app.get_onboarding_open() {
                    return;
                }
                let current = gamepad_name.lock().map(|m| m.clone()).unwrap_or_default();
                let mut e = onboarding_state.lock().unwrap();
                if e.gamepad_name != current {
                    e.gamepad_name = current;
                    refresh_onboarding(&app, &e);
                }
            }
        },
    );

    // Watchers only enqueue hints; reconciliation and all I/O run on workers.
    let inventory_timer = slint::Timer::default();
    inventory_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        {
            let weak = app.as_weak();
            let mut last_fallback = std::time::Instant::now();
            move || {
                if SCAN_RUNNING.load(Ordering::Relaxed) {
                    return;
                }
                if inventory::pending()
                    || last_fallback.elapsed() >= std::time::Duration::from_secs(60)
                {
                    last_fallback = std::time::Instant::now();
                    if let Some(app) = weak.upgrade() {
                        app.invoke_rescan();
                    }
                }
            }
        },
    );

    // Winit applies the initial content size while showing the native window.
    // Give Windows one event-loop interval to deliver the initial resize before
    // maximizing. A zero-duration callback still races that resize on Windows.
    app.window().set_maximized(true);
    app.show().expect("no se pudo mostrar la ventana");
    let startup_window = app.as_weak();
    slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
        if let Some(app) = startup_window.upgrade() {
            app.window().set_maximized(true);
        }
    });

    // #605 — lo que tardó en estar lista la ventana. El escaneo de la
    // biblioteca sigue en su hilo: el CA es sobre lo PERCIBIDO, y una pantalla
    // que ya responde mientras termina de cargar es una pantalla lista.
    eprintln!(
        "[play] arranque:   {} ms (hasta la ventana lista)",
        startup_started.elapsed().as_millis()
    );

    slint::run_event_loop().expect("Slint event loop failed");
    app.hide().expect("no se pudo cerrar la ventana");
    if !SCAN_RUNNING.load(Ordering::Acquire)
        && !PREPARING.load(Ordering::Acquire)
        && install_state
            .lock()
            .is_ok_and(|state| state.phase != install::Phase::Working)
        && let Err(error) = shutdown_store.checkpoint_and_backup()
    {
        log_warning!(
            "catalog",
            "No se pudo completar la copia de cierre: {}",
            error
        );
    }
}

// ---------------------------------------------------------------------------
// #588 — el puente entre el wizard y la ventana.
//
// `refresh_onboarding` vuelca el estado entero a las propiedades cada vez que algo
// cambia. Volcarlo entero y no por partes es a proposito: un refresco parcial
// deja la pantalla mostrando una mezcla de dos estados, que es como se ven los
// defectos que nadie puede reproducir.
// ---------------------------------------------------------------------------
fn onboarding_row(title: String, detail: String) -> OnboardingRow {
    OnboardingRow {
        title: title.into(),
        detail: detail.into(),
        folder: false,
    }
}

fn refresh_onboarding(app: &AppWindow, e: &onboarding::State) {
    app.set_onboarding_step(e.step.index() as i32);
    // #599 — abierto desde Ajustes no hay «Paso 3 de 6»: no es un recorrido de
    // seis pasos y los puntos anunciarían cinco pantallas que no van a venir.
    app.set_onboarding_single_step(e.single_step_mode);
    app.set_onboarding_heading(e.step.heading().into());
    app.set_onboarding_blocker(e.blocker().unwrap_or_default().into());
    // #627 — el pie en PARES (tecla, etiqueta): la UI lo dibuja como el
    // PadGlyph del disenio mas su etiqueta. La tecla es dato y no texto, asi que
    // no viaja al catalogo de traduccion.
    let hints: Vec<PadHint> = e
        .hints()
        .into_iter()
        .map(|(key, label)| PadHint {
            key: key.into(),
            icon: Default::default(),
            label: label.into(),
            // Sin tecla propia de teclado: el wizard usa la gramática general
            // (Enter es A, Esc es B) y `Mando.glifo` ya la dibuja.
            keyboard_key: Default::default(),
        })
        .collect();
    app.set_onboarding_hints(ModelRc::new(VecModel::from(hints)));
    app.set_onboarding_accept(e.terms_accepted);
    // #590 — el texto legal sale de Rust, que es donde se traduce y donde un
    // test lo revisa por enlaces. La UI lo dibuja y no lo escribe.
    let terms: Vec<SharedString> = onboarding::terms_paragraphs()
        .into_iter()
        .map(SharedString::from)
        .collect();
    app.set_onboarding_terms(ModelRc::new(VecModel::from(terms)));
    // Sólo el día, no la hora: en pantalla la hora exacta es ruido; el registro
    // completo con segundos vive en `play_config.toml`, que es donde se
    // consulta cuando importa.
    app.set_onboarding_accept_date(
        e.terms_accepted_at
            .as_deref()
            .map(|f| f.split('T').next().unwrap_or(f).to_string())
            .unwrap_or_default()
            .into(),
    );
    app.set_onboarding_gamepad(e.gamepad_name.as_str().into());
    app.set_onboarding_browsing(e.browsing);
    app.set_onboarding_browse_dirs(e.explorer_folder_mode);
    app.set_onboarding_browse_path(e.explorer_dir.display().to_string().into());

    // Los idiomas como CHIPS (play-ce.pen «00»): los tres se ven a la vez en vez
    // de ciclar a ciegas. Los nombres salen de `nombre_idioma`, que es el mismo
    // que usa el resumen — dos listas de nombres es como terminan diciendo
    // cosas distintas.
    let languages: Vec<SharedString> = onboarding::LANGUAGES
        .iter()
        .map(|c| SharedString::from(onboarding::language_name(c)))
        .collect();
    app.set_onboarding_languages(ModelRc::new(VecModel::from(languages)));
    app.set_onboarding_language(
        onboarding::LANGUAGES
            .iter()
            .position(|c| *c == e.language)
            .unwrap_or(0) as i32,
    );

    let rows: Vec<OnboardingRow> = match e.step {
        onboarding::Step::Welcome => {
            vec![onboarding_row(
                tr!("Idioma"),
                onboarding::language_name(&e.language),
            )]
        }
        onboarding::Step::Folders => e
            .folders
            .iter()
            .zip(e.roms_per_folder.iter())
            .map(|(d, n)| {
                let detail = if *n == 0 {
                    tr!("Sin ROM conocidas")
                } else {
                    tr!("{} archivos", n)
                };
                onboarding_row(d.display().to_string(), detail)
            })
            .collect(),
        onboarding::Step::Summary => e
            .summary()
            .into_iter()
            .map(|(k, v)| onboarding_row(k, v))
            .collect(),
        _ => Vec::new(),
    };
    let row_count = rows.len();
    app.set_onboarding_rows(ModelRc::new(VecModel::from(rows)));

    // El core, con su semaforo y su remedio.
    app.set_onboarding_core(
        e.core
            .as_ref()
            .and_then(|c| c.file_name().map(|f| f.to_string_lossy().to_string()))
            .unwrap_or_default()
            .into(),
    );
    match &e.verdict {
        Some(v) => {
            // TRES estados y no dos. «No se pudo comprobar» NO es
            // «incompatible»: el core puede estar perfecto y el que falta ser el
            // runtime. Decirle incompatible al archivo del jugador por un
            // problema de este lado lo manda a buscar otro core que tampoco va a
            // andar. Es el estado 3 del semáforo —«No verificado»—, que ya
            // existe en la grilla y en la ficha.
            app.set_onboarding_core_state(
                if v.is_usable() {
                    "ok"
                } else if matches!(v, core_probe::Verdict::ProbeFailed(_)) {
                    "unknown"
                } else {
                    "bad"
                }
                .into(),
            );
            app.set_onboarding_core_title(v.title().into());
            app.set_onboarding_core_remedy(v.remedy().into());
            // El detalle es la ficha TÉCNICA del core, y sólo existe cuando se
            // lo pudo abrir. Cuando no, lo que hay que decir —qué pasó y qué
            // hacer— lo dice la alerta de abajo: repetirlo acá era la misma
            // frase tres veces en la misma pantalla.
            app.set_onboarding_core_detail(
                match v {
                    core_probe::Verdict::Valid(i) => {
                        let p: Vec<String> = i.platforms().into_iter().collect();
                        format!("libretro API {} · {}", i.api, p.join(", "))
                    }
                    _ => String::new(),
                }
                .into(),
            );
        }
        None => {
            app.set_onboarding_core_state("".into());
            app.set_onboarding_core_title("".into());
            app.set_onboarding_core_remedy("".into());
            app.set_onboarding_core_detail("".into());
        }
    }

    // #627 — la carpeta se marca con el ICONO `folder`, que es lo que pide el
    // diseño. Antes era una barra final en el nombre: no habia fuente de iconos
    // vendorizada y un glifo geometrico de IBM Plex salia tofu — una columna de
    // cajas vacias al lado de cada fila. La barra se va con el icono: el nombre
    // de la carpeta es el nombre, sin decoracion.
    let entries: Vec<OnboardingRow> = e
        .explorer_entries
        .iter()
        .map(|x| OnboardingRow {
            title: x.name.clone().into(),
            detail: Default::default(),
            folder: x.is_dir && !x.is_parent,
        })
        .collect();
    let n_expl = entries.len();
    app.set_onboarding_browse_rows(ModelRc::new(VecModel::from(entries)));

    // Y el foco, dentro de la lista que se esta mirando. Es una red: los
    // llamadores ya lo devuelven arriba cuando la lista cambia, pero una fila
    // fuera de rango no se resalta en ningun lado y la pantalla se ve muerta.
    let n = if e.browsing { n_expl } else { row_count } as i32;
    app.set_onboarding_row(app.get_onboarding_row().clamp(0, (n - 1).max(0)));
}

// Re-proyecta la biblioteca a la UI. Es UNA función y no la misma cuenta
// repetida en cada callback: filtrar, ordenar, armar el modelo y ajustar la
// selección tienen que pasar SIEMPRE juntos, y el día que falte uno el
// síntoma es una selección que apunta a un juego que ya no está en la lista.
fn refresh_ui(
    app: &AppWindow,
    all_items: &Mutex<Vec<library::Game>>,
    filter: &Mutex<library_view::Filter>,
    order: &Mutex<library_view::Order>,
    cfg: &Mutex<config::Config>,
) {
    let g = all_items.lock().unwrap();
    let f = filter.lock().unwrap();
    let o = *order.lock().unwrap();
    let selected_id = app
        .get_games()
        .row_data(app.get_current_game().max(0) as usize)
        .map(|g| g.id);
    let visible_indices = library_view::visible_indices(&g, &f, o);
    let n = visible_indices.len();
    app.set_platforms(ModelRc::new(VecModel::from(
        library_view::platforms(&g)
            .into_iter()
            .map(slint::SharedString::from)
            .collect::<Vec<_>>(),
    )));
    app.set_total_games(g.iter().filter(|x| !x.hidden).count() as i32);
    app.set_filter_active(f.active());
    // #594 — Inicio se arma acá, con la misma toma de datos.
    //
    // Su texto lo COMPONE Rust y no Slint: mezcla tiempo formateado con el
    // pack, y hacerlo en el `.slint` sería poner en la UI una lógica que allí
    // no se puede probar. `format_play_time` tiene sus tests.
    let hero = library_view::continue_game(&g);
    app.set_has_continue(hero.is_some());
    if let Some(i) = hero {
        let selected_index = [i];
        let m = build_game_model_from(&g, &selected_index, &cfg.lock().unwrap());
        if let Some(e) = m.row_data(0) {
            app.set_continue_game(e);
        }
        // El pack sólo se nombra si lo hay: «· sin pack» es ruido.
        let pack = if g[i].has_pack {
            format!(" · {}", tr!("con pack HD"))
        } else {
            String::new()
        };
        app.set_continue_meta(
            format!(
                "{} {}{}",
                tr!("Jugado:"),
                library_view::format_play_time(g[i].seconds_played),
                pack
            )
            .as_str()
            .into(),
        );
    }
    let rec = library_view::recent(&g, 6);
    app.set_recent(build_game_model_from(&g, &rec, &cfg.lock().unwrap()));
    // Queue Home artwork first so Continue does not wait for the whole library.
    app.set_games(build_game_model_from(
        &g,
        &visible_indices,
        &cfg.lock().unwrap(),
    ));

    // La selección se recorta al nuevo largo. Sin esto, filtrar con el
    // cursor abajo lo deja apuntando fuera de la lista: la tarjeta con foco
    // desaparece y las flechas dejan de responder hasta tocar el mouse.
    if let Some(id) = selected_id
        && let Some(index) = visible_indices.iter().position(|&i| g[i].id == id.as_str())
    {
        app.set_current_game(index as i32);
    }
    if app.get_current_game() >= n as i32 {
        app.set_current_game(if n == 0 { 0 } else { n as i32 - 1 });
    }
}

/// #601: cómo llamar a la ROM de un manifest cuando ya no hay fila.
///
/// El manifest guarda la RUTA, y la ruta de Windows lleva el nombre de usuario
/// adentro. En una pantalla lo que hay que mostrar es el juego, así que se
/// recorta al nombre del archivo sin extensión — que es además lo que el
/// jugador reconoce.
fn rom_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// #599: la etiqueta de procedencia, traducida.
///
/// La tabla vive en `settings.rs` sin traducir —es lógica, y se testea— y la
/// traducción va acá, donde ya está el catálogo. Traducirla allá metería `tr!`
/// en un módulo puro y sus tests dependerían del idioma.
fn translate_origin(o: settings::Origin) -> String {
    match o {
        settings::Origin::Game => tr!("este juego"),
        settings::Origin::Console => tr!("esta consola"),
        settings::Origin::Global => tr!("global"),
        settings::Origin::Pack => tr!("el pack"),
        settings::Origin::Factory => tr!("de fábrica"),
    }
}

// ---------------------------------------------------------------------------
// Library scan
// ---------------------------------------------------------------------------

static PREPARING: AtomicBool = AtomicBool::new(false);
static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
static PREPARE_CANCEL: AtomicBool = AtomicBool::new(false);
static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);
static SCAN_PENDING: AtomicBool = AtomicBool::new(false);
static CATALOG_WRITABLE: AtomicBool = AtomicBool::new(false);

fn scan_progress(weak: slint::Weak<AppWindow>, stage: String, done: usize, total: usize) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            app.set_scan_stage(stage.into());
            app.set_scan_done(done as i32);
            app.set_scan_total(total as i32);
        }
    });
}

/// #591 — el escaneo, en su hilo, cancelable y con progreso.
///
/// `cancel` es el MISMO token que tiene el botón «Cancelar» de la UI: acá se
/// baja a `false` al empezar, porque un token que quedó en `true` de un escaneo
/// anterior haría que el siguiente terminara al instante sin mirar nada.
#[allow(clippy::too_many_arguments)]
fn do_scan(
    weak: slint::Weak<AppWindow>,
    cfg: Arc<Mutex<config::Config>>,
    lib: Arc<Mutex<library::Library>>,
    dat: Arc<Mutex<Option<Arc<nointro::DatFile>>>>,
    cancel: library::CancelToken,
    all_items: Arc<Mutex<Vec<library::Game>>>,
    filter: Arc<Mutex<library_view::Filter>>,
    order: Arc<Mutex<library_view::Order>>,
    _core_build: Arc<Mutex<Option<String>>>,
) {
    if !CATALOG_WRITABLE.load(Ordering::Acquire) {
        return;
    }
    if SCAN_RUNNING.swap(true, Ordering::Relaxed) {
        SCAN_PENDING.store(true, Ordering::Relaxed);
        return;
    }
    cancel.store(false, std::sync::atomic::Ordering::Relaxed);
    slint::invoke_from_event_loop({
        let weak = weak.clone();
        move || {
            if let Some(app) = weak.upgrade() {
                app.set_scanning(true);
                app.set_scan_stage(tr!("Buscando juegos…").into());
                app.set_scan_done(0);
                app.set_scan_total(0);
                app.set_scan_found(0);
                app.set_status(tr!("Escaneando…").into());
            }
        }
    })
    .ok();

    std::thread::spawn(move || {
        let scan_started = std::time::Instant::now();
        let (known_games, store) = {
            let library = lib.lock().unwrap();
            (library.known_games().clone(), library.store())
        };
        let configuration = cfg.lock().unwrap().clone();
        let mut last_progress = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let dat = dat.lock().unwrap().clone();
        let result = catalog_scan::scan(
            store,
            &configuration,
            &known_games,
            dat.as_deref(),
            &cancel,
            &mut |done, total| {
                if last_progress.elapsed() >= std::time::Duration::from_millis(100) || done == total
                {
                    scan_progress(weak.clone(), tr!("Identificando juegos…"), done, total);
                    last_progress = std::time::Instant::now();
                }
            },
        );
        let (games, cancelled, unavailable, mut scan_error, artwork_changes, changed) = match result
        {
            Ok(mut out) => {
                let changed = !out.games.is_empty()
                    || !out.removed.is_empty()
                    || !out.artwork_changes.is_empty();
                let error = {
                    let mut library = lib.lock().unwrap();
                    for game in &mut out.games {
                        if let (Some(before), Some(latest)) = (
                            known_games.get(&game.path),
                            library.known_games().get(&game.path),
                        ) {
                            library::keep_newer_pack_choice(game, before, latest);
                        }
                    }
                    library
                        .apply_scan(&out.games, if out.cancelled { &[] } else { &out.removed })
                        .err()
                };
                (
                    out.games,
                    out.cancelled,
                    out.unavailable,
                    error,
                    out.artwork_changes,
                    changed,
                )
            }
            Err(error) => (
                Vec::new(),
                cancel.load(Ordering::Relaxed),
                Vec::new(),
                Some(error),
                Vec::new(),
                false,
            ),
        };
        if scan_error.is_some() {
            inventory::retry();
        }
        if scan_error.is_none() && !unavailable.is_empty() {
            for detail in &unavailable {
                log_warning!("catalog", "Carpeta no disponible: {}", detail);
            }
            scan_error = Some(tr!(
                "Biblioteca lista. Revisá en Ajustes las carpetas que no están disponibles."
            ));
        }
        // La lista que se muestra sale de la CACHÉ y no de `games`: en un
        // escaneo cancelado `games` es un pedazo, y reemplazar la biblioteca
        // entera por ese pedazo la haría encogerse a la vista del jugador.
        let displayed_games = if changed {
            lib.lock().map(|l| l.get_all()).unwrap_or(games)
        } else {
            Vec::new()
        };
        // #593: los ocultos no se cuentan. El número del estado tiene que decir
        // lo mismo que muestra la pantalla, o parece que se perdieron juegos.
        if changed {
            *all_items.lock().unwrap() = displayed_games;
        }
        let n = all_items
            .lock()
            .unwrap()
            .iter()
            .filter(|g| !g.hidden)
            .count();
        log_info!(
            "play",
            "escaneo completado en {} ms; contadores acumulados: {}",
            scan_started.elapsed().as_millis(),
            cache_metrics::snapshot()
        );
        slint::invoke_from_event_loop(move || {
            SCAN_RUNNING.store(false, Ordering::Relaxed);
            artwork::invalidate(&artwork_changes);
            if let Some(app) = weak.upgrade() {
                // Se re-proyecta CONSERVANDO el filtro: reiniciarlo al terminar
                // un escaneo le borraría al jugador la búsqueda que estaba
                // escribiendo mientras la biblioteca se llenaba.
                if changed {
                    refresh_ui(&app, &all_items, &filter, &order, &cfg);
                }
                app.set_scanning(false);
                if changed && app.get_details_open() {
                    refresh_open_details(&app, &all_items, &cfg);
                }
                app.set_status(
                    if let Some(error) = scan_error {
                        error
                    } else if cancelled {
                        tr!("Escaneo cancelado — {} juegos en la biblioteca", n)
                    } else if n == 1 {
                        tr!("{} juego encontrado", n)
                    } else {
                        tr!("{} juegos encontrados", n)
                    }
                    .into(),
                );
                if SCAN_PENDING.swap(false, Ordering::Relaxed) {
                    app.invoke_rescan();
                }
            }
        })
        .ok();
    });
}

// ---------------------------------------------------------------------------
// Model helpers
// ---------------------------------------------------------------------------

fn render_details_pack_list(app: &AppWindow, games: &[library::Game]) {
    let entry = app.get_details_game();
    let paths = games
        .iter()
        .find(|g| g.path == entry.path.as_str())
        .map(|g| g.available_packs.as_slice())
        .unwrap_or_default();
    let rows: Vec<_> = paths
        .iter()
        .map(|path| InstalledPackEntry {
            path: path.as_str().into(),
            name: std::path::Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
                .into(),
        })
        .collect();
    app.set_details_pack_count(rows.len() as i32);
    app.set_details_selected_pack_present(paths.iter().any(|p| p == entry.pack_path.as_str()));
    app.set_details_packs(ModelRc::new(VecModel::from(rows)));
}

/// Reconcile the visible pack controls without reopening or auditing any pack.
fn refresh_open_details(
    app: &AppWindow,
    games: &Arc<Mutex<Vec<library::Game>>>,
    cfg: &Arc<Mutex<config::Config>>,
) {
    if !app.get_details_open() {
        return;
    }
    let path = app.get_details_game().path;
    let games = games.lock().unwrap();
    let Some(index) = games.iter().position(|g| g.path == path.as_str()) else {
        return;
    };
    let config = cfg.lock().unwrap();
    let Some(entry) = build_game_model_from(&games, &[index], &config).row_data(0) else {
        return;
    };
    app.set_details_game(entry);
    render_details_pack_list(app, &games);
    let game = &games[index];
    let core = cores::cached_state(&config, &game.platform);
    let axes = game_details::axes(
        &game.compat_codes,
        game.has_pack,
        game.pack_status,
        &game.compat_reason,
        &core,
        &game.platform,
    );
    app.set_details_pack_status(axes.pack.state.as_int());
    app.set_details_pack_reason(axes.pack_reason().into());
    app.set_details_pack_usable(axes.pack_usable());
    app.set_details_pack_warning(axes.short_pack_reason().into());
    app.set_details_pack_usage_status(
        game_details::PackState::of(game.has_pack, axes.pack_usable(), app.get_details_hd())
            .as_int(),
    );
    if !app.get_details_selected_pack_present() {
        app.invoke_details_cancel();
        app.set_pack_name("".into());
        app.set_pack_author("".into());
        app.set_pack_audit("".into());
        app.set_pack_audit_ok(false);
    }
}

/// #678 — el espejo del diálogo de packs, entero en cada cambio: fila, fase,
/// lista y resultado. Pintar de a pedazos es como una fase vieja queda en
/// pantalla con la lista nueva.
fn render_install(app: &AppWindow, e: &install::State) {
    app.set_install_open(e.is_open);
    app.set_install_folder_mode(e.mode == install::Mode::Folder);
    app.set_install_cover_mode(e.mode == install::Mode::Cover);
    app.set_install_cover_error(e.cover_error.as_str().into());
    app.set_install_path(e.dir.to_string_lossy().as_ref().into());
    app.set_install_row(e.row);
    let rows: Vec<OnboardingRow> = e
        .entries
        .iter()
        .map(|x| OnboardingRow {
            title: x.name.as_str().into(),
            detail: "".into(),
            folder: x.is_dir,
        })
        .collect();
    app.set_install_rows(ModelRc::new(VecModel::from(rows)));
    match &e.phase {
        install::Phase::Browsing => app.set_install_phase(0),
        install::Phase::Working => app.set_install_phase(1),
        install::Phase::Outcome(r) => {
            app.set_install_title(r.title.as_str().into());
            app.set_install_detail(r.detail.as_str().into());
            app.set_install_can_revert(r.can_revert);
            app.set_install_phase(2);
        }
    }
}

/// El espejo del diálogo de controles, entero en cada cambio.
fn render_controls(app: &AppWindow, d: &controls::Dialog, c: &controls::Controls) {
    app.set_controls_open(d.is_open);
    app.set_controls_row(d.row);
    app.set_controls_capture(match d.capture {
        controls::Capture::None => 0,
        controls::Capture::Key => 1,
        controls::Capture::Button => 2,
    });
    let rows: Vec<ControlRow> = controls::ACTIONS
        .iter()
        .map(|a| ControlRow {
            action: controls::action_label(a.id).as_str().into(),
            key: controls::key_label(&c.key_of(a.id)).as_str().into(),
            button: c
                .button_of(a.id)
                .map(|b| controls::button_label(&b))
                .unwrap_or_else(|| "—".into())
                .as_str()
                .into(),
            // La cruceta no se remapea: la fila lo dice apagando la columna, no
            // fallando al apretar X.
            no_button: a.default_button.is_none(),
        })
        .collect();
    app.set_controls_rows(ModelRc::new(VecModel::from(rows)));
}

/// El X del diálogo, con el lock acotado: elegir la carpeta actual y soltar
/// antes de tocar la configuración, que tiene su propio candado.
fn selected_install_source(inst: &Mutex<install::State>) -> Option<std::path::PathBuf> {
    inst.lock().unwrap().select_current_folder()
}

/// #311: lo que espera para lanzarse mientras la pantalla previa esta abierta.
struct PendingLaunch {
    reservations: Vec<Arc<std::fs::File>>,
    rom: std::path::PathBuf,
    /// #596: el CRC32 de la ROM, capturado al elegirla. Va al manifest y es lo
    /// que lo hace verificable: la ruta puede apuntar a otro archivo mañana.
    crc32: String,
    pack: Option<std::path::PathBuf>,
    caps: launch_config::PackCapabilities,
    cfg: launch_config::LaunchConfig,
    file: std::path::PathBuf,
    name: String,
    /// #604 — desde qué guardado reanudar. `None` = partida limpia.
    ///
    /// **Es el CA de la issue**: Continuar reanuda exactamente el último
    /// guardado, y Jugar no lo pisa. Lo segundo es cierto por el layout de
    /// #603 —cada salida escribe una versión NUEVA, nunca encima— así que una
    /// partida limpia no puede borrar la anterior aunque el jugador la
    /// abandone en el primer nivel.
    resume_from: Option<std::path::PathBuf>,
    /// #601 — ¿esta config se recuerda para la próxima vez?
    ///
    /// `false` sólo en el modo seguro. **Es un defecto que costaría caro**: sin
    /// esto, arrancar una vez en modo seguro guardaría «sin pack, sin shaders»
    /// como la configuración del juego, y todos los lanzamientos siguientes
    /// serían seguros sin que nadie lo pidiera — con el pack instalado, el
    /// perfil elegido, y ninguna pista de por qué no se ven.
    remember_configuration: bool,
}

/// Las filas del modelo, ya formateadas. El que decide QUE dice cada fila es
/// `launch_config`, que tiene tests; aca solo se copian a la vista.
/// #315: `--install <pack.ay> [--id <id>]` · `--rollback <id>` · `--packs`.
/// #614: `--uninstall <id>` · #622: `--diagnostico [<sesión>] [--guardar]`
/// · #620: `--exportar <zip>` · `--importar <zip>`.
///
/// Devuelve `Some(código)` cuando atendió un comando (y el proceso termina sin
/// abrir la ventana), o `None` para seguir al launcher normal.
fn run_pack_cli(cfg: &config::Config) -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    // La grafia vieja en espanol se acepta como sinonimo: un script del
    // jugador que decia --exportar no se rompe por la normalizacion.
    let flag2 = |a: &str, b: &str| flag(a).or_else(|| flag(b));
    let val = |i: usize| args.get(i + 1).cloned();
    let dir = &cfg.packs_install_dir;
    if ["--install", "--rollback", "--uninstall"]
        .iter()
        .any(|name| flag(name).is_some())
    {
        match catalog::Store::open(&cfg.catalog_path()) {
            Ok(store) => pack_cache::init(store),
            Err(error) => {
                eprintln!("{error}");
                return Some(1);
            }
        }
        if let Some(repo) = pack_cache::global()
            && let Err(error) = repo
                .require_writable()
                .and_then(|_| repo.recover_promotions())
        {
            eprintln!("{error}");
            return Some(1);
        }
    }

    if let Some(i) = flag("--install") {
        let Some(path) = val(i).map(std::path::PathBuf::from) else {
            eprintln!("uso: ayther_play_ce --install <pack.ay> [--id <id>]");
            return Some(2);
        };
        // El id por defecto es el nombre del archivo: es lo que el jugador ve
        // y lo que la biblioteca usa. `--id` existe para reinstalar sobre un
        // id ya instalado con otro nombre de archivo.
        let id = flag("--id").and_then(val).unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "pack".into())
        });
        return Some(match pack_install::install_local(dir, &id, &path) {
            pack_install::InstallResult::Installed { replaced } => {
                log_info!("play", "instalado {id} en {}", dir.display());
                if replaced {
                    println!(
                        "La versión anterior quedó conservada                               (--rollback {id} vuelve a ella)."
                    );
                }
                0
            }
            pack_install::InstallResult::Invalid(m) => {
                eprintln!("Pack inválido ({m}): no se instaló. Lo instalado sigue intacto.");
                1
            }
            pack_install::InstallResult::Io(m) => {
                eprintln!("Error: {m}");
                1
            }
        });
    }

    if let Some(i) = flag("--rollback") {
        let Some(id) = val(i) else {
            eprintln!("uso: ayther_play_ce --rollback <id>");
            return Some(2);
        };
        return Some(match pack_install::rollback(dir, &id) {
            Ok(()) => {
                println!("{id}: se volvió a la versión anterior.");
                0
            }
            Err(e) => {
                eprintln!("{id}: {e}");
                1
            }
        });
    }

    if let Some(i) = flag("--uninstall") {
        let Some(id) = val(i) else {
            eprintln!("uso: ayther_play_ce --uninstall <id>");
            return Some(2);
        };
        return Some(match pack_install::uninstall(dir, &id) {
            pack_install::UninstallResult::Uninstalled { had_previous } => {
                println!(
                    "{id}: desinstalado{}. La ROM, el core y los guardados quedan                           donde estaban.",
                    if had_previous {
                        " (con su versión anterior)"
                    } else {
                        ""
                    }
                );
                0
            }
            pack_install::UninstallResult::NotInstalled => {
                eprintln!("{id}: no hay ningún pack instalado con ese id.");
                1
            }
            pack_install::UninstallResult::Io(e) => {
                eprintln!("{id}: {e}");
                1
            }
        });
    }

    if let Some(i) = flag2("--export", "--exportar") {
        let Some(destination) = val(i).map(std::path::PathBuf::from) else {
            eprintln!("uso: ayther_play_ce --export <archivo.zip>");
            return Some(2);
        };
        let saves = cfg.saves_dir.clone();
        return Some(
            match porting::export(&saves, Some(&config::Config::config_file()), &destination) {
                Ok(r) => {
                    println!(
                        "Exportados {} guardado(s){}. Es un ZIP normal: se abre, se copia                           y se importa en otra máquina sin Play de por medio.",
                        r.saves,
                        if r.preferences {
                            " y tus preferencias"
                        } else {
                            ""
                        }
                    );
                    log_info!("play", "export en {}", r.file.display());
                    0
                }
                Err(e) => {
                    eprintln!("No se pudo exportar: {e}");
                    1
                }
            },
        );
    }

    if let Some(i) = flag2("--import", "--importar") {
        let Some(origin) = val(i).map(std::path::PathBuf::from) else {
            eprintln!("uso: ayther_play_ce --importar <archivo.zip>");
            return Some(2);
        };
        let saves = cfg.saves_dir.clone();
        let when = date::compact_timestamp();
        return Some(match porting::import(&origin, &saves, &when) {
            Ok(r) => {
                println!(
                    "Se agregaron {} save(s); {} ya estaban; {} se saltearon.
                          Importar AGREGA: lo que tenías sigue donde estaba.",
                    r.saves, r.already_present, r.skipped
                );
                0
            }
            Err(e) => {
                eprintln!("No se pudo importar: {e}");
                1
            }
        });
    }

    if let Some(i) = flag2("--diagnostics", "--diagnostico") {
        // Se IMPRIME, y sólo se guarda si lo piden: la garantía de #622 es que
        // el jugador lea lo que va a mandar antes de que exista un archivo que
        // alguien pueda adjuntar sin mirar.
        let session = val(i)
            .filter(|v| !v.starts_with("--"))
            .map(|id| cfg.sessions_dir().join("sessions").join(id));
        let diag = cfg.diagnostics_dir();
        let p = diagnostics::package(env!("CARGO_PKG_VERSION"), session.as_deref(), &diag);
        println!("{}", p.text);
        if flag2("--save", "--guardar").is_some() {
            return Some(match diagnostics::save(&diag, &p.text) {
                Ok(d) => {
                    log_info!("play", "diagnóstico guardado en {}", d.display());
                    0
                }
                Err(e) => {
                    log_error!("play", "no se pudo guardar el diagnóstico: {e}");
                    1
                }
            });
        }
        println!("(agregá --guardar para dejarlo en un archivo y adjuntarlo vos)");
        return Some(0);
    }

    if flag("--packs").is_some() {
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("ay") {
                    continue;
                }
                let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                    continue;
                };
                let prev = if pack_install::has_previous(dir, &stem) {
                    "  (hay versión anterior)"
                } else {
                    ""
                };
                println!("{stem}{prev}");
                n += 1;
            }
        }
        if n == 0 {
            log_info!("play", "no hay packs instalados en {}", dir.display());
        }
        return Some(0);
    }
    None
}

/// #312: la ficha del pack, convertida a filas para la pantalla.
///
/// El armado vive acá y no en la UI porque el ORDEN es parte del mensaje: se
/// muestra primero lo comprobado —la auditoría del archivo y la compatibilidad,
/// que es lo que decide si vale la pena seguir leyendo— y después lo declarado.
/// Al revés, el jugador lee tres párrafos de autoría antes de enterarse de que
/// el pack no es para su juego.
fn build_detail_model(d: &pack_details::PackDetails) -> ModelRc<DetailRow> {
    let mut rows: Vec<DetailRow> = Vec::new();
    let mut push = |label: &str, value: &str, level: &str| {
        rows.push(DetailRow {
            label: label.into(),
            value: value.into(),
            level: level.into(),
        });
    };

    // -- Comprobado ----------------------------------------------------------
    //
    // La fila dice «Archivo» y no «Firma» a propósito: es lo que CE comprobó.
    // Una fila llamada «Firma» con un «no se pudo verificar» al lado se lee
    // como un problema del pack, y no lo es — es un límite de esta edición.
    push("COMPROBADO", "", "");
    push(
        "Archivo",
        &d.audit.word(),
        match d.audit {
            verifier::Audit::Complete => "ok",
            verifier::Audit::MissingIndex => "advertencia",
            _ => "error",
        },
    );

    let mb = d.size_bytes as f64 / (1024.0 * 1024.0);
    push("Tamaño", &format!("{mb:.1} MB"), "");

    // Los hallazgos del validador, con su nivel. Van con el CÓDIGO delante: es
    // lo que hace buscable un problema — «rom.mismatch» encuentra respuestas,
    // «no coincide» no.
    for f in &d.findings {
        push(&format!("  {}", f.code), &f.message, f.level);
    }
    if d.findings.is_empty() {
        push("Revisión", "Sin hallazgos", "ok");
    }

    // -- Declarado -----------------------------------------------------------
    push("DECLARADO POR EL AUTOR", "", "");
    if !d.version.is_empty() {
        push("Versión", &d.version, "");
    }
    if !d.author.is_empty() {
        push("Autor", &d.author, "");
    }
    if !d.contributors.is_empty() {
        push("Colaboradores", &d.contributors.join(", "), "");
    }
    if !d.license.is_empty() {
        push("Licencia", &d.license, "");
    } else {
        // La ausencia se DICE: sin licencia declarada, quien lo descargue no
        // sabe qué puede hacer con el contenido. Dejar la fila afuera haría
        // que pareciera un dato que nadie pidió.
        push("Licencia", "no declarada", "advertencia");
    }
    if !d.description.is_empty() {
        push("Descripción", &d.description, "");
    }
    if !d.game_id.is_empty() {
        push("Juego", &d.game_id, "");
    }

    if !d.profiles.is_empty() {
        let names: Vec<&str> = d.profiles.iter().map(|(_, n)| n.as_str()).collect();
        push("Perfiles", &names.join(" · "), "");
    }

    // `[systems]` AUSENTE no es `[systems]` vacío (#294): un pack viejo no
    // declara nada, y decir «no sustituye nada» sobre uno que sustituye todo
    // sería mentirle al jugador con cara de dato.
    if d.systems_declared {
        let s = if d.systems.is_empty() {
            "ninguno".to_string()
        } else {
            d.systems.join(", ")
        };
        push("Sustituye", &s, "");
    } else {
        push("Sustituye", "el pack no lo declara (formato anterior)", "");
    }

    if !d.credits.is_empty() {
        push("CRÉDITOS", "", "");
        for c in &d.credits {
            let detail = if c.role.is_empty() {
                c.contribution.clone()
            } else {
                format!("{} · {}", c.role, c.contribution)
            };
            push(
                &format!("  {}", c.author),
                detail.trim_end_matches(" · "),
                "",
            );
        }
    }

    ModelRc::new(VecModel::from(rows))
}

/// #314: los créditos del pack, para el panel de la pantalla previa. Sin pack
/// (o sin `credits.toml`) la lista queda vacía — el panel dice que el pack no
/// declara créditos en vez de mostrar una lista muda.
fn build_credit_model(pack: Option<&std::path::Path>) -> ModelRc<CreditLineData> {
    let lines: Vec<CreditLineData> = pack
        .map(pack_meta::credits)
        .unwrap_or_default()
        .into_iter()
        .map(|c| CreditLineData {
            author: c.author.into(),
            role: c.role.into(),
            contribution: c.contribution.into(),
            licenses: c.licenses.into(),
        })
        .collect();
    ModelRc::new(VecModel::from(lines))
}

fn build_config_model(
    caps: &launch_config::PackCapabilities,
    cfg: &launch_config::LaunchConfig,
) -> ModelRc<ConfigRowData> {
    let rows: Vec<ConfigRowData> = launch_config::config_rows(caps, cfg)
        .into_iter()
        .map(|r| ConfigRowData {
            label: r.label.into(),
            value: r.value.into(),
            available: r.available,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// #592: `cfg` entra porque la tarjeta muestra si HAY core para la plataforma,
/// y eso no vive en la biblioteca — vive en la configuración del jugador. Se
/// resuelve acá, al armar el modelo, y no dentro de `Game`: es un dato del
/// entorno, no del archivo, y guardarlo en la caché lo dejaría rancio en cuanto
/// alguien configure un core nuevo.
/// #593: el modelo de la UI se arma sólo con los ÍNDICES visibles.
///
/// La lista se recalcula en cada tecla de la búsqueda, así que armarla desde los
/// índices mantiene barata esa recalculación. Las carátulas se piden a la caché:
/// las que faltan se decodifican en su worker sin bloquear el event loop.
struct LazyGameModel {
    games: Vec<library::Game>,
    cfg: config::Config,
    notify: slint::ModelNotify,
}
impl LazyGameModel {
    fn cover_changed(&self, path: &str) {
        for (index, game) in self.games.iter().enumerate() {
            if game.artwork() == path {
                self.notify.row_changed(index);
            }
        }
    }
}
impl Model for LazyGameModel {
    type Data = GameEntry;
    fn row_count(&self) -> usize {
        self.games.len()
    }
    fn row_data(&self, index: usize) -> Option<GameEntry> {
        self.games.get(index).map(|g| game_entry(g, &self.cfg))
    }
    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &self.notify
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
fn game_entry(g: &library::Game, cfg: &config::Config) -> GameEntry {
    let artwork = g.artwork();
    GameEntry {
        id: g.id.as_str().into(),
        name: g.title().into(),
        path: g.path.as_str().into(),
        pack_path: g.pack_path.as_str().into(),
        platform: g.platform.as_str().into(),
        crc32: g.crc32.as_str().into(),
        region: g.region.as_str().into(),
        has_pack: g.has_pack,
        pack_status: g.pack_status.as_int(),
        cover: if artwork.is_empty() {
            slint::Image::default()
        } else {
            artwork::load(std::path::Path::new(&artwork))
        },
        cover_path: artwork.into(),
        core_state: cores::cached_state(cfg, &g.platform).as_int(),
        favorite: g.favorite,
        hidden: g.hidden,
        edited: g.has_edits(),
        compat_reason: g.compat_reason.as_str().into(),
    }
}
fn build_game_model_from(
    games: &[library::Game],
    visible_indices: &[usize],
    cfg: &config::Config,
) -> ModelRc<GameEntry> {
    ModelRc::new(LazyGameModel {
        games: visible_indices
            .iter()
            .filter_map(|&i| games.get(i).cloned())
            .collect(),
        cfg: cfg.clone(),
        notify: slint::ModelNotify::default(),
    })
}
/// Escribe todo pánico a un archivo, además de al stderr de siempre.
///
/// El archivo lleva la hora en el nombre y no se pisa: dos cierres seguidos son
/// dos datos, y el segundo suele ser el que se puede reproducir. El hook
/// anterior se sigue llamando —es el que imprime el mensaje estándar— para no
/// perder el formato al que ya está acostumbrado quien lee el log.
///
/// `RUST_BACKTRACE=1` sigue mandando sobre el backtrace: acá se guarda lo que
/// el runtime de Rust decida mostrar, sin forzar nada.
fn install_panic_hook(dir: &std::path::Path) {
    let dir = dir.to_path_buf();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Primero lo de siempre: si esto fallara, el mensaje se perdería.
        previous(info);
        let current_thread = std::thread::current();
        let text = format!(
            "ayther_play_ce {}\nhora:   {}\nhilo:   {}\ndonde:  {}\nque:    {info}\n",
            env!("CARGO_PKG_VERSION"),
            date::now_utc(),
            current_thread.name().unwrap_or("<sin nombre>"),
            info.location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_else(|| "<desconocido>".into()),
        );
        if std::fs::create_dir_all(&dir).is_ok() {
            let file = dir.join(format!("play-panico-{}.log", date::compact_timestamp()));
            let _ = std::fs::write(&file, &text);
            log_error!("play", "pánico registrado en {}", file.display());
        }
    }));
}

// ---------------------------------------------------------------------------
// #586 — el puente entre la ventana y el enrutador del mando.
//
// `estado_del_pad` copia los flags que deciden el ruteo; `aplicar_efecto`
// traduce la orden a llamadas de Slint. Todo lo que decide QUÉ pasa vive en
// `pad_router`, que no conoce Slint y por eso se puede probar.
// ---------------------------------------------------------------------------
fn pad_state(app: &AppWindow) -> pad_router::State {
    pad_router::State {
        confirm_open: app.get_confirm_open(),
        playing: app.get_playing(),
        onboarding: app.get_onboarding_open(),
        browsing: app.get_onboarding_browsing(),
        onboarding_row: app.get_onboarding_row(),
        onboarding_rows: if app.get_onboarding_browsing() {
            app.get_onboarding_browse_rows().row_count() as i32
        } else {
            app.get_onboarding_rows().row_count() as i32
        },
        crash_open: app.get_crash_open(),
        orphan_open: app.get_orphan_open(),
        survey_open: app.get_feedback_open(),
        credits_open: app.get_credits_open(),
        details_open: app.get_detail_open() || app.get_details_open(),
        saves_open: app.get_saves_open(),
        install_open: app.get_install_open(),
        controls_open: app.get_controls_open(),
        save_row: app.get_saves_row(),
        saves: app.get_saves_list().row_count() as i32,
        config_open: app.get_config_open(),
        tab: app.get_active_tab(),
        game: app.get_current_game(),
        games: app.get_games().row_count() as i32,
        // #629 — el paso de arriba/abajo en la biblioteca. La cuenta la hace la
        // ventana, que es la que conoce su ancho; en vista de lista es 1, que es
        // lo que una lista significa: una columna.
        columns: if app.get_list_view() {
            1
        } else {
            app.get_columns()
        },
        setting_row: app.get_setting_row(),
        // #599 — la cuenta la tiene `settings.rs`, que es donde también se dice
        // qué hace cada fila. Dos cuentas separadas es cómo la última fila
        // termina sin poder enfocarse.
        settings_count: settings::ROWS,
        config_row: app.get_config_row(),
        config_row_count: app.get_config_rows().row_count() as i32,
        survey_row: app.get_feedback_row(),
        survey_row_count: app.get_feedback_rows().row_count() as i32,
    }
}

fn apply_effect(app: &AppWindow, ef: pad_router::Effect) {
    use pad_router::Effect::*;
    match ef {
        NoOp => {}
        SelectGame(i) => app.set_current_game(i),
        Launch(i) => app.invoke_launch(i),
        // Cambiar de seccion SALE de la ficha: es navegacion del chasis, y una
        // seccion nueva con el juego todavia en el cuerpo es la barra diciendo
        // una cosa y la pantalla otra.
        GoToTab(t) => {
            app.set_active_tab(t);
            app.set_details_open(false);
        }
        SafeMode => {
            app.set_crash_open(false);
            app.invoke_crash_safe_mode();
        }
        CloseCrash => app.set_crash_open(false),
        PlayFromDetails => app.invoke_details_play(),
        InstallFromDetails => app.invoke_details_install_pack(),
        NextPack => app.invoke_details_next_pack(),
        UninstallPack => app.invoke_details_uninstall_pack(app.get_details_game().pack_path),
        ConfirmAccept => {
            app.invoke_confirm_accept();
            app.set_confirm_open(false);
        }
        ConfirmCancel => app.set_confirm_open(false),
        NewGame => app.invoke_details_play_clean(),
        ToggleHD => app.invoke_details_toggle_hd(),
        SaveRowAt(i) => app.set_saves_row(i),
        SettingsRowAt(i) => app.set_setting_row(i),
        ActivateSetting(i) => app.invoke_setting_activate(i),
        SettingSecondary(i) => app.invoke_setting_secondary(i),
        // #678 — el diálogo de packs: el gesto viaja y `install.rs` decide.
        InstallMove(d) => app.invoke_install_move(d),
        InstallActivate => app.invoke_install_activate(app.get_install_row()),
        InstallAction => app.invoke_install_action(),
        InstallRevert => app.invoke_install_revert(),
        InstallClose => app.invoke_install_close(),
        ControlsMove(d) => app.invoke_controls_move(d),
        ControlsKey => app.invoke_controls_capture_key(),
        ControlsButton => app.invoke_controls_capture_button(),
        ControlsRestore => app.invoke_controls_restore(),
        ControlsClose => app.invoke_controls_close(),
        RestoreSave(i) => app.invoke_saves_restore(i),
        CloseSaves => app.set_saves_open(false),
        RecoverSession => app.invoke_orphan_recover(),
        DiscardSession => app.invoke_orphan_discard(),
        ConfigRowAt(i) => app.set_config_row(i),
        AdjustConfig(row, d) => app.invoke_config_adjust(row, d),
        AcceptConfig => app.invoke_config_accept(),
        CancelConfig => app.invoke_config_cancel(),
        // La ficha de JUEGO (#597) y el informe del PACK (#312) comparten
        // superficie —las dos son «ficha» para el ruteo—, asi que se cierra la
        // que este arriba. Antes esto cerraba SIEMPRE el informe: en la ficha
        // de juego, «B Atras» —que el pie anuncia— no hacia nada, y con la
        // ficha ademas tapando la TopBar no quedaba ninguna forma de volver a
        // la biblioteca.
        CloseDetails => {
            if app.get_detail_open() {
                app.set_detail_open(false);
            } else {
                app.set_details_open(false);
            }
        }
        CloseCredits => app.set_credits_open(false),
        SurveyRow(i) => app.set_feedback_row(i),
        Rate(row, d) => app.invoke_feedback_rate(row, d),
        SaveSurvey => app.invoke_feedback_send(),
        SurveyLater => app.invoke_feedback_later(),
        // #588 — el wizard. `onboarding-row` es lo unico que la UI mueve sola; el
        // resto se lo pide a Rust, que es donde estan las reglas.
        OnboardingRowAt(i) => app.set_onboarding_row(i),
        OnboardingActivate => app.invoke_onboarding_activate(),
        OnboardingNext => app.invoke_onboarding_next(),
        OnboardingBack => app.invoke_onboarding_back(),
        OnboardingSecondary(d) => app.invoke_onboarding_secondary(d),
        OnboardingStepSecondary => app.invoke_onboarding_activate_secondary(),
        OnboardingStepAction => app.invoke_onboarding_step_action(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UN HOOK QUE NO ESCRIBE NO SIRVE DE NADA, y es justo el que nadie prueba:
    /// se descubre el día del cierre que había que diagnosticar, cuando ya no
    /// hay nada que leer. Acá se provoca un pánico de verdad y se comprueba que
    /// quedó el archivo con lo que hace falta para empezar a buscar.
    #[test]
    fn a_panic_leaves_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let previous = std::panic::take_hook();
        install_panic_hook(dir.path());

        let r = std::panic::catch_unwind(|| panic!("prueba del hook"));
        std::panic::set_hook(previous); // que no se lo lleve el resto de la suite
        assert!(r.is_err());

        let panic_logs: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("play-panico-"))
            .collect();
        assert_eq!(panic_logs.len(), 1, "el pánico tiene que dejar UN archivo");

        let text = std::fs::read_to_string(panic_logs[0].path()).unwrap();
        // Lo mínimo para poder buscar: qué pasó, dónde y en qué hilo.
        assert!(
            text.contains("prueba del hook"),
            "sin el mensaje no se busca nada: {text}"
        );
        assert!(text.contains("main.rs"), "falta el archivo:línea: {text}");
        assert!(text.contains("hilo:"), "falta el hilo: {text}");
    }
}
