const SLINT_COMPILER_STACK_SIZE: usize = 16 * 1024 * 1024;

fn compile_slint_ui() -> Result<(), String> {
    // El compilador de Slint recorre la UI de forma profundamente recursiva. En Windows,
    // compilar appwindow.slint en el hilo principal de build.rs agotaba su stack por defecto
    // (STATUS_STACK_OVERFLOW). Un hilo dedicado permite fijar un límite suficiente sin cambiar
    // el stack del binario final.
    // #585 — traducciones EMPAQUETADAS: slint-build lee `lang/<idioma>/LC_MESSAGES/ayther_play_ce.po`
    // y las mete en el binario. Sin gettext en runtime (su `gettext-sys` compila
    // libintl desde fuente en Windows) y sin archivos sueltos que instalar.
    //
    // `DefaultTranslationContext::None`: por defecto Slint usa el nombre del
    // componente como contexto del msgid; sin esto, cada texto tendría que
    // repetirse por componente en el .po y una refactorización de la UI
    // invalidaría las traducciones en silencio.
    let compiler = std::thread::Builder::new()
        .name("slint-compiler".into())
        .stack_size(SLINT_COMPILER_STACK_SIZE)
        .spawn(|| {
            let cfg = slint_build::CompilerConfiguration::new()
                .with_bundled_translations("lang")
                .with_default_translation_context(slint_build::DefaultTranslationContext::None);
            slint_build::compile_with_config("ui/appwindow.slint", cfg)
                .map_err(|error| format!("Slint UI compile failed: {error}"))
        })
        .map_err(|error| format!("no se pudo iniciar el compilador de Slint: {error}"))?;

    compiler
        .join()
        .map_err(|_| "el compilador de Slint terminó con panic".to_string())?
}

fn main() {
    compile_slint_ui().unwrap_or_else(|error| panic!("{error}"));

    // Icono del .exe (Explorer, barra de tareas anclada, Alt-Tab). Es OTRO icono
    // que el de la ventana: Windows lo lee del recurso ICON del ejecutable, así
    // que existe antes de que la app abra y sobrevive a que esté cerrada. El de
    // la ventana lo pone Slint desde el PNG (`icon` del Window). Mismo patrón
    // —y mismo `.rc`— que lab/resources/ayther_lab.rc.
    //
    // Se compila SÓLO para targets Windows: el job de Linux de release.yml
    // construye el mismo crate y ahí no hay recursos Win32 que emitir.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/ayther_play_ce.rc", embed_resource::NONE)
            .manifest_required()
            .expect("no se pudo compilar resources/ayther_play_ce.rc");
    }
    // Cambiar el .ico tiene que rehacer el recurso: sin esto el .res queda
    // rancio y el icono del exe sigue siendo el anterior hasta un build limpio
    // (el mismo problema que en el Lab resuelve OBJECT_DEPENDS).
    println!("cargo:rerun-if-changed=resources/ayther_play_ce.rc");
    println!("cargo:rerun-if-changed=resources/AYTHER-Play.ico");
    // Y los CATÁLOGOS, por lo mismo. Declarar cualquier `rerun-if-changed`
    // apaga el default de cargo —«rehacé si cambió algo del paquete»—, así que
    // sin esta línea regenerar `lang/` con `gen_translations.py` no rehacía el
    // bundle: el .po tenía la traducción nueva, el binario seguía con las
    // viejas, y en pantalla se veía el texto en español dentro de la UI en
    // inglés. Se falla en silencio y sólo se nota mirando.
    println!("cargo:rerun-if-changed=lang");
}
