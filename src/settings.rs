//! settings.rs — de dónde sale cada valor (#599).
//!
//! # El CA es «todo valor muestra de dónde sale»
//!
//! Un ajuste con cuatro lugares posibles donde definirse —global, por consola,
//! por juego, y lo que el pack recomienda— es un ajuste que el jugador cambia y
//! no pasa nada, porque otro más específico lo estaba pisando. Sin decir de
//! dónde sale el valor, la única manera de entenderlo es probar hasta que
//! funcione, y ahí el ajuste deja de ser un control y pasa a ser una lotería.
//!
//! # El orden, y por qué el pack va abajo
//!
//! Gana el MÁS ESPECÍFICO: **juego › consola › global › pack › fábrica**.
//!
//! El pack va por debajo de lo global aunque parezca más específico, y es
//! deliberado: el pack RECOMIENDA (`output`, #296), no impone. Quien mira la
//! pantalla es el que sabe si tiene un OLED o un proyector, y un pack que
//! pisara una preferencia del jugador convertiría su recomendación en una orden
//! — con el efecto de que instalar un pack le cambia los ajustes a alguien que
//! no pidió nada.
//!
//! Fábrica va último porque es el único que siempre existe: es lo que hace que
//! `resolver` no pueda devolver «no sé».

/// De dónde salió el valor que se está mostrando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Game,
    Console,
    Global,
    Pack,
    Factory,
}

impl Origin {
    /// ¿Hay algo que RESTAURAR? Sólo tiene sentido ofrecer «volver al valor
    /// heredado» cuando el valor de acá lo está pisando; en los otros casos el
    /// botón no haría nada y su presencia sugiere que sí.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn can_restore(self) -> bool {
        matches!(self, Origin::Game | Origin::Console)
    }
}

/// Un valor y su procedencia.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved<T> {
    pub value: T,
    pub origin: Origin,
}

/// Resuelve un ajuste por precedencia. `None` = no definido en ese nivel.
///
/// El `fabrica` no es opcional: es lo que garantiza que siempre haya respuesta.
/// Con todos opcionales, la función tendría que devolver `Option` y cada
/// llamador inventaría su propio default — que es como cuatro pantallas terminan
/// mostrando cuatro valores distintos del mismo ajuste.
pub fn resolve<T>(
    game: Option<T>,
    console: Option<T>,
    global: Option<T>,
    pack: Option<T>,
    factory: T,
) -> Resolved<T> {
    if let Some(v) = game {
        return Resolved {
            value: v,
            origin: Origin::Game,
        };
    }
    if let Some(v) = console {
        return Resolved {
            value: v,
            origin: Origin::Console,
        };
    }
    if let Some(v) = global {
        return Resolved {
            value: v,
            origin: Origin::Global,
        };
    }
    if let Some(v) = pack {
        return Resolved {
            value: v,
            origin: Origin::Pack,
        };
    }
    Resolved {
        value: factory,
        origin: Origin::Factory,
    }
}

/// Lo que una acción destructiva **conserva**, en palabras.
///
/// Existe como función y no como texto suelto en cada diálogo porque el CA es
/// que la confirmación diga qué se conserva, y un texto por diálogo es un texto
/// que alguien va a escribir de memoria — y va a olvidarse justo del que
/// importaba. Acá están juntos y se leen los tres de una.
///
/// En CE son tres y no cuatro: la que borraba las copias de la nube se fue con
/// la nube (ADR-005). Era además la única que destruía algo que no se podía
/// volver a armar desde el disco del jugador — acá no hay ninguna así.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destructive {
    // Las dos primeras todavía no las arma ninguna pantalla —la única que
    // confirma hoy es la bienvenida—, pero sus textos ya están escritos y
    // probados: es la mitad cara de la feature y la que no se improvisa cuando
    // el botón llegue. El `allow` no cubre `cargo test`, así que si el oráculo
    // que las recorre se va, el aviso vuelve.
    #[cfg_attr(not(test), allow(dead_code))]
    RemoveFolder,
    #[cfg_attr(not(test), allow(dead_code))]
    RemoveCore,
    RedoWelcome,
}

impl Destructive {
    pub fn title(self) -> &'static str {
        match self {
            Destructive::RemoveFolder => "¿Quitar esta carpeta?",
            Destructive::RemoveCore => "¿Quitar este core?",
            Destructive::RedoWelcome => "¿Volver a ver la bienvenida?",
        }
    }

    /// Qué se conserva. **Es la parte que importa**: sin ella, «quitar carpeta»
    /// se lee como «borrar mis ROMs», y quien tiene su biblioteca ahí no lo
    /// aprieta nunca — o lo aprieta y pasa un mal rato.
    pub fn preserved_items(self) -> &'static str {
        match self {
            Destructive::RemoveFolder => {
                "No se borra ningún archivo. Tus ROMs, tus packs y tus guardados \
                 quedan donde están; sólo dejan de aparecer en la biblioteca."
            }
            Destructive::RemoveCore => {
                "No se borra el archivo del core. Los juegos de esa consola van a \
                 quedar sin core hasta que elijas otro."
            }
            Destructive::RedoWelcome => {
                "No se borra nada: ni tu biblioteca, ni tus carpetas, ni tus \
                 guardados. Volvés a pasar por los pasos y podés cambiar lo que \
                 quieras."
            }
        }
    }

    /// El texto del botón que confirma. Dice la ACCIÓN y no «Aceptar»: en un
    /// diálogo de dos botones, «Aceptar» obliga a volver a leer el título para
    /// saber qué se está aceptando.
    pub fn confirm(self) -> &'static str {
        match self {
            Destructive::RemoveFolder => "Quitar la carpeta",
            Destructive::RemoveCore => "Quitar el core",
            Destructive::RedoWelcome => "Ver la bienvenida",
        }
    }
}

// ---------------------------------------------------------------------------
// Qué hace cada fila (#599)
// ---------------------------------------------------------------------------

/// Las filas de «05 · Ajustes», en el orden en que se dibujan.
///
/// El número vive acá y no en `main.rs` ni en `pad_router`: el enrutador del
/// mando necesita saber cuántas filas hay para moverse entre ellas, y dos
/// cuentas separadas es cómo la última fila termina sin poder enfocarse.
///
/// **Son menos que en AYTHER Play, y ésa es la diferencia entre las ediciones
/// escrita donde se dibuja.** Se fueron la cuenta, la nube, la telemetría, el
/// borrado de las copias remotas, el canal de actualización y volver a la
/// versión anterior: las seis hablan con un servicio o con lo que ese servicio
/// instaló, y CE no tiene ninguno (ADR-005).
pub const ROWS: i32 = 10;

/// Lo que «A» hace sobre una fila.
///
/// Existe como dato y no como un `match` adentro del callback de Slint por lo
/// mismo que `pad_router`: acá está la decisión, así que se puede probar que
/// **ninguna fila activable no hace nada**. Ése fue el defecto: las seis filas
/// se dibujaban, se enfocaban y anunciaban «A Cambiar», y cinco de las seis
/// caían en un `if fila == 5` — el jugador apretaba A sobre «Idioma» o sobre
/// «Carpetas de ROMs» y la pantalla no se movía.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Rota al siguiente idioma y lo aplica en caliente.
    CycleLanguage,
    /// Abre el paso de carpetas del wizard, que es el mismo que el del primer
    /// arranque: dos selectores de carpeta distintos serían dos
    /// comportamientos distintos para la misma decisión.
    EditFolders,
    /// Ídem con el paso del core.
    EditCores,
    /// Rota al siguiente perfil de salida global.
    CycleOutput,
    /// Abre el diálogo de controles del juego: qué tecla y qué botón hace
    /// cada acción del RetroPad en la partida. Va con lo del sistema —idioma,
    /// cores, salida— porque es la misma pregunta: cómo se usa esta máquina.
    OpenControls,
    /// #678 — designa la carpeta de packs: lo que aparezca ahí queda disponible
    /// sin instalar, como una ROM en la carpeta de ROMs. Abre el explorador en
    /// modo carpeta; la secundaria de la fila ([`secundaria`]) la quita.
    ChoosePacksFolder,
    /// #678 — abre el explorador filtrado a `.ay` e instala el elegido por el
    /// MISMO camino que `--install`: auditoría antes de tocar nada, versión
    /// anterior conservada, revertir a mano.
    InstallPack,
    /// #620 — escribe el ZIP con los guardados y las preferencias. Un jugador
    /// que no puede sacar sus partidas está atrapado.
    ///
    /// En CE no es una comodidad: **es la única copia de respaldo que existe**,
    /// porque no hay nube que la haga sola. Por eso queda, y queda a la vista.
    Export,
    /// La fila es informativa: no promete nada y no hace nada.
    Info,
    /// Pide confirmación antes de hacer algo que no se deshace.
    Confirm(Destructive),
}

/// Qué hace «A» sobre la fila `i`. Fuera de rango, nada: el enrutador clampea,
/// pero un índice viejo puede llegar igual desde un click.
pub fn action(row: i32) -> Option<Action> {
    Some(match row {
        0 => Action::CycleLanguage,
        1 => Action::EditFolders,
        2 => Action::EditCores,
        3 => Action::CycleOutput,
        4 => Action::OpenControls,
        // #678 — los packs van con el CONTENIDO, pegados a las carpetas de ROMs
        // y a los cores: son la misma pregunta («¿de dónde salen mis cosas?»)
        // y no una acción de mantenimiento del final de la lista.
        5 => Action::ChoosePacksFolder,
        6 => Action::InstallPack,
        // «Carpeta de datos» se muestra y no se edita: mudarla es mover los
        // guardados de lugar, y eso no es un ajuste de una fila.
        7 => Action::Info,
        // Llevarse los datos, y recién al final lo que confirma. Lo destructivo
        // no va arriba de lo cotidiano.
        8 => Action::Export,
        9 => Action::Confirm(Destructive::RedoWelcome),
        _ => return None,
    })
}

/// La acción SECUNDARIA de una fila (Y del mando), o `None` si no tiene.
///
/// Hoy hay una sola: quitar la carpeta de packs. Vive como dato por lo mismo
/// que [`accion`]: que se pueda probar que Y no hace algo distinto de lo que la
/// fila anuncia — y que quitar NO borre nada, que es lo que el texto de la fila
/// promete: los `.ay` quedan donde están, sólo dejan de aparecer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryAction {
    RemovePacksFolder,
}

pub fn secondary_action(row: i32) -> Option<SecondaryAction> {
    ((0..ROWS).contains(&row) && action(row) == Some(Action::ChoosePacksFolder))
        .then_some(SecondaryAction::RemovePacksFolder)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gana el más específico, y el orden es el contrato: si «global» le ganara
    /// a «este juego», cambiar un ajuste desde la ficha no haría nada y no habría
    /// manera de darse cuenta salvo probando.
    #[test]
    fn the_most_specific_wins() {
        let r = resolve(Some("j"), Some("c"), Some("g"), Some("p"), "f");
        assert_eq!(r.value, "j");
        assert_eq!(r.origin, Origin::Game);

        assert_eq!(
            resolve(None, Some("c"), Some("g"), Some("p"), "f").origin,
            Origin::Console
        );
        assert_eq!(
            resolve(None, None, Some("g"), Some("p"), "f").origin,
            Origin::Global
        );
        assert_eq!(
            resolve(None, None, None, Some("p"), "f").origin,
            Origin::Pack
        );
        assert_eq!(
            resolve(None::<&str>, None, None, None, "f").origin,
            Origin::Factory
        );
    }

    /// EL PACK VA DEBAJO DE LO GLOBAL, aunque parezca más específico.
    ///
    /// El pack recomienda, no impone: quien mira la pantalla es el que sabe si
    /// tiene un OLED o un proyector. Al revés, instalar un pack le cambiaría los
    /// ajustes a alguien que no pidió nada.
    #[test]
    fn the_pack_recommends_but_does_not_stomp_the_player() {
        let r = resolve(None, None, Some("lcd"), Some("crt"), "pixel");
        assert_eq!(r.value, "lcd", "manda lo que eligio el jugador");
        assert_eq!(r.origin, Origin::Global);

        // Y sin elección suya, sí manda el pack — para eso recomienda.
        let r = resolve(None, None, None, Some("crt"), "pixel");
        assert_eq!(r.value, "crt");
        assert_eq!(r.origin, Origin::Pack);
    }

    /// Siempre hay respuesta: `fabrica` no es opcional. Con todos opcionales
    /// cada llamador inventaría su default, que es como cuatro pantallas
    /// terminan mostrando cuatro valores distintos del mismo ajuste.
    #[test]
    fn there_is_always_a_value() {
        let r = resolve(None::<i32>, None, None, None, 42);
        assert_eq!(r.value, 42);
        assert_eq!(r.origin, Origin::Factory);
    }

    /// «Restaurar» sólo se ofrece cuando hay algo que restaurar. Un botón que no
    /// hace nada sugiere que sí lo hace, y el jugador lo aprieta esperando un
    /// cambio que nunca llega.
    #[test]
    fn restore_only_where_there_is_something_to_undo() {
        assert!(Origin::Game.can_restore());
        assert!(Origin::Console.can_restore());
        assert!(!Origin::Global.can_restore());
        assert!(!Origin::Pack.can_restore());
        assert!(!Origin::Factory.can_restore());
    }

    /// NINGUNA FILA ACTIVABLE HACE NADA. Es el oráculo del defecto: las seis
    /// filas se dibujaban y anunciaban «A Cambiar», pero sólo la última estaba
    /// cableada — el jugador apretaba A sobre «Idioma» y la pantalla no se
    /// movía, que es indistinguible de un mando que no responde.
    #[test]
    fn every_row_does_something() {
        for i in 0..ROWS {
            let a = action(i).unwrap_or_else(|| panic!("la fila {i} no existe"));
            if i == 7 {
                assert_eq!(a, Action::Info, "la carpeta de datos no se edita");
            } else {
                assert_ne!(a, Action::Info, "la fila {i} promete y no hace nada");
            }
        }
        assert_eq!(action(ROWS), None, "no hay fila después de la última");
        assert_eq!(action(-1), None);
    }

    /// Sólo confirman las filas DESTRUCTIVAS. Si otra empezara a pedir
    /// confirmación sin serlo, el diálogo dejaría de significar «cuidado».
    #[test]
    fn only_the_destructive_ones_confirm_and_they_go_last() {
        let confirmation_rows: Vec<i32> = (0..ROWS)
            .filter(|i| matches!(action(*i), Some(Action::Confirm(_))))
            .collect();
        // Los índices se DERIVAN, no se escriben: un oráculo atado a números de
        // fila se rompe cada vez que la pantalla crece, y a la tercera vez
        // alguien lo borra en lugar de mirarlo.
        assert_eq!(
            confirmation_rows,
            vec![ROWS - 1],
            "una destructiva se metió entre las filas de todos los días: {confirmation_rows:?}"
        );
    }

    /// EL ORÁCULO DE LA EDICIÓN: ninguna fila de Ajustes de CE habla con un
    /// servicio, ni con lo que un servicio instaló.
    ///
    /// No alcanza con haber borrado las filas una vez. La lista crece, y la
    /// manera natural de agregarle «Cuenta» o «Buscar actualizaciones» es
    /// copiarlas de Play, donde funcionan. Acá no funcionarían —no hay con
    /// quién hablar— y el jugador se encontraría con una fila que no hace nada.
    /// Este test obliga a que agregarlas sea una decisión y no un brazo más del
    /// `match`.
    #[test]
    fn no_ce_row_needs_a_service() {
        for i in 0..ROWS {
            let a = action(i).unwrap();
            assert!(
                matches!(
                    a,
                    Action::CycleLanguage
                        | Action::EditFolders
                        | Action::EditCores
                        | Action::CycleOutput
                        | Action::OpenControls
                        | Action::ChoosePacksFolder
                        | Action::InstallPack
                        | Action::Export
                        | Action::Info
                        | Action::Confirm(_)
                ),
                "la fila {i} hace {a:?}, que en CE no tiene con quién hablar (ADR-005)"
            );
        }
    }

    /// #678 — LOS PACKS ENTRAN DESDE AJUSTES, y las dos filas van JUNTAS y con
    /// el contenido: la carpeta y el instalador son los dos caminos del mismo
    /// pack, y separados se leen como dos features que compiten.
    #[test]
    fn the_two_pack_rows_exist_and_go_together() {
        let folder = (0..ROWS)
            .find(|i| action(*i) == Some(Action::ChoosePacksFolder))
            .expect("no está la fila de la carpeta de packs");
        let install_row = (0..ROWS)
            .find(|i| action(*i) == Some(Action::InstallPack))
            .expect("no está la fila de instalar un pack");
        assert_eq!(
            install_row,
            folder + 1,
            "los dos caminos del pack van juntos"
        );
    }

    /// La secundaria (Y) sólo existe donde hay algo secundario que hacer, y lo
    /// único que hace es DEJAR DE MIRAR una carpeta — no borra nada. Si otra
    /// fila ganara una Y, este test obliga a decidirlo acá y no en el callback.
    #[test]
    fn the_only_secondary_one_is_removing_the_packs_folder() {
        let rows_with_secondary_action: Vec<i32> = (0..ROWS)
            .filter(|i| secondary_action(*i).is_some())
            .collect();
        assert_eq!(
            rows_with_secondary_action.len(),
            1,
            "filas con secundaria: {rows_with_secondary_action:?}"
        );
        assert_eq!(
            action(rows_with_secondary_action[0]),
            Some(Action::ChoosePacksFolder)
        );
        assert_eq!(
            secondary_action(rows_with_secondary_action[0]),
            Some(SecondaryAction::RemovePacksFolder)
        );
        assert_eq!(secondary_action(-1), None);
        assert_eq!(secondary_action(ROWS), None);
    }

    /// Y LA FILA QUE NO SE PUEDE IR: sin nube, exportar es el único respaldo.
    ///
    /// En Play, perderla sería perder una comodidad —las partidas están también
    /// en el servidor—. Acá sería quedarse sin ninguna forma de sacar una
    /// partida de la máquina, y eso hay que romperlo a propósito.
    #[test]
    fn export_is_still_there_because_it_is_the_only_backup() {
        assert!(
            (0..ROWS).any(|i| action(i) == Some(Action::Export)),
            "sin nube, exportar es la única copia de respaldo que hay"
        );
    }

    /// EL OTRO CA: toda confirmación destructiva dice qué se CONSERVA, y lo dice
    /// nombrando los archivos. Sin eso, «quitar carpeta» se lee como «borrar mis
    /// ROMs» y quien tiene su biblioteca ahí no lo aprieta nunca.
    #[test]
    fn every_confirmation_says_what_is_kept() {
        for d in [
            Destructive::RemoveFolder,
            Destructive::RemoveCore,
            Destructive::RedoWelcome,
        ] {
            assert!(!d.title().is_empty());
            assert!(
                !d.preserved_items().is_empty(),
                "{d:?} no dice que conserva"
            );
            // El botón dice la ACCIÓN, no «Aceptar»: con dos botones, «Aceptar»
            // obliga a releer el título para saber qué se acepta.
            assert!(!d.confirm().eq_ignore_ascii_case("aceptar"));
            assert!(d.confirm().len() > 6, "«{}» no dice que hace", d.confirm());
        }
        // Y la de la carpeta nombra lo que a nadie le da lo mismo perder.
        let c = Destructive::RemoveFolder.preserved_items();
        for word in ["ROMs", "guardados", "No se borra"] {
            assert!(c.contains(word), "falta «{word}»: {c}");
        }
    }
}
