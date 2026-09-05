//! Los oráculos de #642. Ninguno necesita ventana, runtime ni un core de verdad:
//! el sondeo entra como dato, que es justamente lo que permite probar los casos
//! que en esta máquina no existen (un core de 32 bits, uno de otra consola).

use super::*;
use crate::core_probe::{Architecture, CoreInfo};
use crate::library::PackStatus;

/// Un core que declara estas extensiones y nada más.
fn valid(exts: &[&str]) -> Verdict {
    Verdict::Valid(CoreInfo {
        api: 1,
        name: "Core De Prueba".into(),
        version: "v1".into(),
        extensions: exts.iter().map(|e| e.to_string()).collect(),
    })
}

/// Una config con un core que EXISTE en el disco, para que la pregunta que se
/// pruebe sea la del sondeo y no la del archivo.
fn with_core(dir: &std::path::Path) -> (Config, PathBuf) {
    let core = dir.join("core.dll");
    std::fs::write(&core, b"x").unwrap();
    let mut c = Config::defaults();
    c.default_core = core.clone();
    (c, core)
}

// ---------------------------------------------------------------------------
// EL CA de #642
// ---------------------------------------------------------------------------

/// **Un core que el sondeo rechaza no da «Compatible».**
///
/// Es el defecto entero: `hay_core_para` decía que sí porque el archivo estaba,
/// y la ficha lo mostraba como jugable. Los tres rechazos que el sondeo sabe
/// distinguir tienen que llegar a la ficha como incompatibles y con un remedio
/// que se pueda leer.
#[test]
fn a_core_the_probe_rejects_does_not_show_compatible_on_the_card() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, core) = with_core(dir.path());

    let rejected_verdicts = [
        Verdict::ForeignArchitecture {
            core: Architecture::X86,
            runtime: Architecture::X64,
        },
        Verdict::NotACore(core.clone()),
        Verdict::ForeignApi { api: 2 },
        // Y el que sólo se puede ver sondeando: carga, es un core, pero es de
        // otra consola. Con el archivo puesto y la ruta escrita, esto era
        // «Compatible».
        valid(&["sfc", "smc"]),
    ];

    for v in rejected_verdicts {
        let mut s = Probes::new();
        s.record(core.clone(), v.clone());
        let e = axis(&cfg, &s, "Mega Drive");

        assert!(e.blocks_launch(), "{v:?} dejó jugable a Mega Drive: {e:?}");
        assert_eq!(e.as_int(), 1, "{v:?} tendría que ser «no sirve»");

        // Y lo que se muestra en la ficha es un eje incompatible, no un
        // «no verificado» que se lee como «puede que sí».
        let axes = crate::game_details::axes(&[], false, PackStatus::NoPack, "", &e, "Mega Drive");
        assert_eq!(axes.core.state, PackStatus::Incompatible, "{v:?}");
        assert!(
            axes.core.reason.len() > 20,
            "{v:?} no dice qué hacer: «{}»",
            axes.core.reason
        );
    }
}

/// El caso bueno, para que el test de arriba no pase por estar todo roto.
#[test]
fn the_core_the_probe_approves_does_show_compatible() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, core) = with_core(dir.path());
    let mut s = Probes::new();
    s.record(core, valid(&["md", "gen", "bin", "sms"]));

    assert_eq!(axis(&cfg, &s, "Mega Drive"), CoreAxis::Usable);
    assert_eq!(axis(&cfg, &s, "Master System"), CoreAxis::Usable);
    let axes = crate::game_details::axes(
        &[],
        false,
        PackStatus::NoPack,
        "",
        &axis(&cfg, &s, "Mega Drive"),
        "Mega Drive",
    );
    assert_eq!(axes.core.state, PackStatus::Exact);
}

// ---------------------------------------------------------------------------
// «Todavía no» no es «no»
// ---------------------------------------------------------------------------

/// Mientras el sondeo no terminó, el eje dice «no verificado» y **deja jugar**.
///
/// Sin esto, el arranque pintaría la biblioteca entera de rojo durante el
/// segundo que tarda el proceso del sondeo, y el jugador vería «Falta el core»
/// de todo lo que tiene.
#[test]
fn while_the_probe_is_not_done_it_neither_blocks_nor_claims() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, _) = with_core(dir.path());
    let e = axis(&cfg, &Probes::new(), "Mega Drive");

    assert_eq!(e, CoreAxis::Unchecked);
    assert!(
        !e.blocks_launch(),
        "un sondeo que no terminó no puede impedir jugar"
    );
    let axes = crate::game_details::axes(&[], false, PackStatus::NoPack, "", &e, "Mega Drive");
    assert_eq!(axes.core.state, PackStatus::Unverified);
}

/// Y que Ayther no pueda comprobar tampoco es un «no».
///
/// `NoSePudoSondear` es un problema de la instalación de Ayther —falta el
/// runtime, o murió sin decir nada—. Bloquear ahí sería castigar al jugador por
/// un defecto nuestro; lo que corresponde es no afirmar nada y decir qué revisar.
#[test]
fn not_being_able_to_check_is_not_the_same_as_rejecting() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, core) = with_core(dir.path());
    let mut s = Probes::new();
    s.record(
        core,
        Verdict::ProbeFailed("no se pudo ejecutar el runtime".into()),
    );

    let e = axis(&cfg, &s, "Mega Drive");
    assert_eq!(e, CoreAxis::CheckFailed);
    assert!(!e.blocks_launch());
    let axes = crate::game_details::axes(&[], false, PackStatus::NoPack, "", &e, "Mega Drive");
    assert_eq!(axes.core.state, PackStatus::Unverified);
    // Y lo que se muestra NO lleva el detalle: nombra el ejecutable que no se
    // pudo correr, y esa ruta lleva el nombre de usuario. El detalle va al log.
    assert!(
        !axes.core.reason.contains("runtime"),
        "el motivo filtró el detalle del fallo: «{}»",
        axes.core.reason
    );
    assert!(
        axes.core.reason.contains("instalación"),
        "y tiene que decir qué revisar: «{}»",
        axes.core.reason
    );
}

/// Un core que carga y es libretro pero no declara extensiones conocidas: no
/// sabemos qué corre. El onboarding lo acepta (#589) y acá tampoco puede
/// bloquear — pero tampoco se afirma que sirva.
#[test]
fn a_core_that_does_not_say_what_it_runs_is_neither_accepted_nor_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (cfg, core) = with_core(dir.path());
    let mut s = Probes::new();
    s.record(core, valid(&[]));

    let e = axis(&cfg, &s, "Mega Drive");
    assert_eq!(e, CoreAxis::Undeclared);
    assert!(!e.blocks_launch());
    let axes = crate::game_details::axes(&[], false, PackStatus::NoPack, "", &e, "Mega Drive");
    assert_eq!(axes.core.state, PackStatus::Unverified);
}

// ---------------------------------------------------------------------------
// El archivo, y de qué core estamos hablando
// ---------------------------------------------------------------------------

/// #592, que sigue valiendo: «hay core» NO es «hay una ruta escrita». Una ruta a
/// un core borrado se contesta sin sondear, porque el sondeo de ayer no dice
/// nada del archivo de hoy.
#[test]
fn a_path_to_a_missing_file_is_core_missing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut cfg, core) = with_core(dir.path());

    // Sondeado y aprobado… y después el archivo desapareció.
    let mut s = Probes::new();
    s.record(core.clone(), valid(&["md"]));
    assert_eq!(axis(&cfg, &s, "Mega Drive"), CoreAxis::Usable);
    std::fs::remove_file(&core).unwrap();
    assert_eq!(
        axis(&cfg, &s, "Mega Drive"),
        CoreAxis::Missing,
        "un sondeo viejo no puede sostener a un archivo que ya no está"
    );

    // Y con el mapa lleno, una plataforma que no está NO se salva por el
    // default: el wizard anota una entrada por cada consola que el core sirve
    // de verdad (#589), así que «no está en el mapa» significa «no la corre».
    std::fs::write(&core, b"x").unwrap();
    cfg.cores.insert("Mega Drive".into(), core.clone());
    assert_eq!(
        axis(&cfg, &s, "SNES"),
        CoreAxis::Missing,
        "el mapa de cores manda: el default no puede desmentirlo"
    );
}

/// El sondeo es del CORE: un core anotado bajo tres consolas es UN proceso.
/// Lanzar uno por tarjeta es lo que #595 evitó.
#[test]
fn the_probe_does_not_repeat_per_platform_or_per_game() {
    let dir = tempfile::tempdir().unwrap();
    let (mut cfg, core) = with_core(dir.path());
    let other = dir.path().join("otro.dll");
    std::fs::write(&other, b"x").unwrap();

    cfg.cores.insert("Mega Drive".into(), core.clone());
    cfg.cores.insert("Master System".into(), core.clone());
    cfg.cores.insert("Game Gear".into(), core.clone());
    cfg.cores.insert("SNES".into(), other.clone());

    let mut paths = to_probe(&cfg);
    paths.sort();
    let mut expected = vec![core, other, cfg.default_core.clone()];
    expected.sort();
    expected.dedup();
    assert_eq!(paths, expected, "una ruta = un sondeo");

    // Y una ruta que no está no se sondea: ya se contesta sin proceso.
    cfg.cores
        .insert("N64".into(), dir.path().join("no-esta.dll"));
    assert!(!to_probe(&cfg).iter().any(|r| r.ends_with("no-esta.dll")));
}

// ---------------------------------------------------------------------------
// El número que cruza a Slint
// ---------------------------------------------------------------------------

/// `bloquea()` sale del número, y son exactamente los dos primeros.
#[test]
fn only_missing_and_unusable_prevent_playing() {
    let all_items = [
        CoreAxis::Missing,
        CoreAxis::Unusable("x".into()),
        CoreAxis::Unchecked,
        CoreAxis::CheckFailed,
        CoreAxis::Undeclared,
        CoreAxis::Usable,
    ];
    // Sin repetidos: dos estados con el mismo número serían el mismo estado
    // para la UI.
    let mut ns: Vec<i32> = all_items.iter().map(|e| e.as_int()).collect();
    let n = ns.len();
    ns.sort();
    ns.dedup();
    assert_eq!(ns.len(), n, "hay dos estados con el mismo número");
    assert_eq!(
        ns,
        (0..n as i32).collect::<Vec<_>>(),
        "los números tienen que ser 0..n"
    );

    for e in &all_items {
        assert_eq!(e.blocks_launch(), e.as_int() <= 1, "{e:?}");
    }
    assert!(CoreAxis::Missing.blocks_launch() && CoreAxis::Unusable("x".into()).blocks_launch());
    assert!(!CoreAxis::Unchecked.blocks_launch());
}

/// **La UI usa los mismos números que este enum.**
///
/// El eje del core cruza a Slint como un entero y allá se decide con él si la
/// tarjeta se apaga y si «Jugar» está habilitado. Son dos listas en dos
/// lenguajes que tienen que decir lo mismo, y nada del compilador las ata: este
/// oráculo lee el `.slint` como texto, igual que el de la precedencia de
/// superficies del mando.
#[test]
fn the_ui_uses_the_same_numbers_as_this_enum() {
    let slint = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/appwindow.slint"),
    )
    .expect("no se pudo leer ui/appwindow.slint");

    // La pregunta que hace la UI para apagar la tarjeta y deshabilitar «Jugar».
    assert!(
        slint.contains("pure public function core-blocks(core: int) -> bool {"),
        "la UI ya no tiene `core-blocks`: alguien la reescribió sin mirar acá"
    );
    assert!(
        slint.contains("core <= 1"),
        "`core-blocks` dejó de ser «los dos primeros»; si el enum cambió, cambiá los dos"
    );

    // Y las dos palabras que dependen del número exacto. Las demás las decide el
    // grado del pack, no el core.
    for (n, word) in [(0, "Falta el core"), (1, "El core no sirve")] {
        let expected = format!("core == {n} ? @tr(\"{word}\")");
        assert!(
            slint.contains(&expected),
            "el .slint no dice `{expected}`: el número {n} cambió de significado"
        );
    }
    // El `bool` viejo no puede haber quedado en ningún lado: si quedó, hay una
    // superficie contestando con el archivo y no con el sondeo.
    assert!(
        !slint.contains("sin-core"),
        "quedó un `sin-core` en la UI — ése era el eje que salía de que el archivo existiera"
    );
}

/// **Un semáforo que no habla del core no puede decir «Falta el core».**
///
/// El defecto, visto en pantalla y no deducido: al pasar de `bool` a `int`, los
/// badges que sólo gradúan un pack —la fila de un pack del Hub, la ficha del
/// pack, el sondeo del onboarding— heredaron el `0` de un `int` sin asignar, y
/// `0` es «Falta el core». La ficha de un juego con core llegó a mostrar el chip
/// rojo «Falta el core» al lado del texto «Hay un core que corre Mega Drive».
///
/// Un `bool` sin asignar es `false`, que era inofensivo; un `int` sin asignar es
/// el peor estado del enum. Por eso el default es explícito y vive fuera del
/// rango: no es «sirve», es «no viene al caso».
#[test]
fn a_traffic_light_that_does_not_speak_of_the_core_stays_quiet() {
    let slint = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/appwindow.slint"),
    )
    .expect("no se pudo leer ui/appwindow.slint");

    assert!(
        slint.contains("out property <int> core-not-applicable: -1;"),
        "sin un valor de «no viene al caso», el default de un badge es 0 = «Falta el core»"
    );
    // Y toda propiedad `core` de un componente arranca con él: un
    // `in property <int> core;` pelado es el defecto de vuelta.
    let properties_without_defaults = slint.matches("in property <int> core;").count();
    assert_eq!(
        properties_without_defaults, 0,
        "hay {properties_without_defaults} propiedad(es) `core` sin default: sin asignar valen 0 = «Falta el core»"
    );
    assert_eq!(
        slint
            .matches("in property <int> core: StatusLight.core-not-applicable;")
            .count(),
        2,
        "el badge y la banda son los dos que lo necesitan"
    );

    // Y del lado de Rust, que ningún estado real caiga en ese valor.
    for e in [
        CoreAxis::Missing,
        CoreAxis::Unusable("x".into()),
        CoreAxis::Unchecked,
        CoreAxis::CheckFailed,
        CoreAxis::Undeclared,
        CoreAxis::Usable,
    ] {
        assert_ne!(
            e.as_int(),
            -1,
            "{e:?} choca con el «no viene al caso» de la UI"
        );
    }
}
