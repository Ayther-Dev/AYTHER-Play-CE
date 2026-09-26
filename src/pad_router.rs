//! pad_router.rs — a dónde va cada botón del mando (#586, PLAY-EP04.3).
//!
//! **El problema.** El ruteo del mando vivía como un `if` en cascada dentro del
//! callback de gilrs y cubría dos superficies: la lista y la configuración
//! previa. Las otras —ficha, créditos, encuesta, el aviso de una sesión sin
//! cerrar— sólo se podían usar con mouse o teclado, en un launcher cuya primera
//! invariante de producto es «todo lo cotidiano se hace con mando» (spec §6).
//! Peor: eso no se ve probando, se ve cuando alguien está sentado en el sillón
//! y una pantalla no responde.
//!
//! **La solución.** Una función pura: `(estado de la UI, botón) → efecto`. El
//! estado es un puñado de flags que ya existen en la ventana y el efecto es una
//! orden que `main.rs` traduce a llamadas de Slint. Así el ruteo se prueba sin
//! abrir una ventana ni conectar un mando, que es la única forma de verificar
//! todas las superficies por todos los botones sin volverse loco.
//!
//! **Las dos reglas de la spec §9 que esto hace cumplir:**
//! - el foco es **persistente y visible**: cada superficie recuerda su fila y
//!   ninguna acción deja el cursor en la nada;
//! - **Atrás nunca cierra la aplicación**: sube un nivel, y en la raíz no hace
//!   nada. Cerrar Play sin querer, con el mando, desde el sillón, es la clase
//!   de error que no se perdona.

use crate::gamepad::PadAction;

/// Qué superficie manda ahora mismo. El orden del `match` de `superficie()`
/// ES la precedencia: lo más modal primero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Confirmation,
    /// El Runtime tiene la pantalla: Play no escucha nada.
    Playing,
    /// #588 — el explorador de archivos del onboarding, encima del wizard.
    Explorer,
    /// #588 — el wizard de primer arranque. Va antes que todo lo demás
    /// (salvo el juego) porque hasta que no termina no hay biblioteca que usar:
    /// es la única pantalla viva.
    Onboarding,
    /// #601 — «el juego se cerró inesperadamente». Va antes que la encuesta:
    /// las dos aparecen al volver de una sesión y preguntarle a alguien qué le
    /// pareció el pack justo después de que se le cayó el juego es preguntar
    /// por el crash.
    Crash,
    /// #604 — el historial de guardados de un juego, sobre la ficha.
    Saves,
    /// El diálogo de controles del juego, sobre Ajustes. Su fila y su fase
    /// viven en `controls::Dialogo`; el ruteo sólo manda el gesto — y cuando
    /// el diálogo está CAPTURANDO un botón, el gesto ni llega: el hilo del
    /// mando lo come antes (`gamepad::start`, `captura`).
    Controls,
    /// #678 — el diálogo de packs: elegir un `.ay` e instalarlo, o designar la
    /// carpeta de packs. Se abre sobre Ajustes o sobre la ficha y tapa a las
    /// dos: mientras se navega el disco, nada de atrás puede moverse.
    InstallPack,
    /// #601 — la sesión que quedó sin cerrar. Aparece al arrancar y **antes que
    /// el wizard no**: si el wizard está, no hubo biblioteca de la cual venir.
    OrphanSession,
    Survey,
    Credits,
    GameDetails,
    PreLaunch,
    /// #599/#605 — Ajustes, que en CE es la sección 2: son tres y no seis.
    Settings,
    Library,
    /// #594 — Inicio: «Continuar» y los recientes. Es la sección 0 y la
    /// que Play abre, así que va ÚLTIMA en esta lista por la misma razón que
    /// `Biblioteca`: la precedencia es de arriba hacia abajo y ser la pantalla
    /// de fondo significa que cualquier cosa la tapa.
    Home,
}

/// Los flags de la ventana que deciden el ruteo. Es un espejo plano de las
/// propiedades de `AppWindow`, para poder razonar (y testear) sin Slint.
#[derive(Debug, Clone, Copy, Default)]
pub struct State {
    pub confirm_open: bool,
    pub playing: bool,
    /// #588 — el wizard de primer arranque está en pantalla.
    pub onboarding: bool,
    /// #588 — y encima de él, el explorador de archivos.
    pub browsing: bool,
    /// Fila enfocada del paso del wizard, y cuántas hay. Sirve para las dos
    /// listas que tiene: las carpetas y las entradas del explorador.
    pub onboarding_row: i32,
    pub onboarding_rows: i32,
    /// #601 — el diálogo del crash y el de la sesión sin cerrar.
    pub crash_open: bool,
    pub orphan_open: bool,
    pub survey_open: bool,
    pub credits_open: bool,
    pub details_open: bool,
    /// #604 — «03d · Guardados», abierto sobre la ficha.
    pub saves_open: bool,
    /// #678 — el diálogo de packs está abierto. Su cursor y su fase NO están
    /// acá: viven en `install::Estado`, y el ruteo sólo manda el gesto — el
    /// mismo reparto que el login de Play y por el mismo motivo.
    pub install_open: bool,
    /// El diálogo de controles del juego está abierto.
    pub controls_open: bool,
    pub save_row: i32,
    pub saves: i32,
    pub config_open: bool,
    pub tab: i32,
    pub game: i32,
    pub games: i32,
    /// #629 — cuántas tarjetas entran en una fila de la grilla. **1 en vista de
    /// lista**, que es lo que una lista significa: una columna.
    ///
    /// Lo aporta la ventana (`AppWindow.columnas`, derivada del ancho útil y de
    /// `CardMetrics`) y no se recalcula acá: el ruteo no conoce el ancho de la
    /// ventana, y una segunda cuenta sería el mando saltando tres tarjetas donde
    /// la grilla dibuja seis. Un 0 —estado sin construir— se lee como 1, porque
    /// un paso de cero deja arriba/abajo sin mover nada.
    pub columns: i32,
    /// #605 — la fila enfocada de Ajustes, y cuántas hay.
    pub setting_row: i32,
    pub settings_count: i32,
    pub config_row: i32,
    pub config_row_count: i32,
    pub survey_row: i32,
    pub survey_row_count: i32,
}

/// Lo que hay que hacer. `main.rs` lo traduce a llamadas de Slint; acá no se
/// toca la UI, y por eso esto se puede probar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    NoOp,
    // Biblioteca
    SelectGame(i32),
    Launch(i32),
    // Navegación entre secciones
    GoToTab(i32),
    // Configuración previa
    ConfigRowAt(i32),
    AdjustConfig(i32, i32),
    AcceptConfig,
    CancelConfig,
    // Ficha y créditos (sub-superficies de la configuración previa)
    CloseDetails,
    CloseCredits,
    // Encuesta
    SurveyRow(i32),
    Rate(i32, i32),
    SaveSurvey,
    SurveyLater,
    // #605 — Ajustes
    SettingsRowAt(i32),
    ActivateSetting(i32),
    /// #678 — Y sobre una fila de Ajustes: su acción secundaria, si tiene
    /// (`settings::secundaria`). Hoy: quitar la carpeta de packs.
    SettingSecondary(i32),
    // #678 — el diálogo de packs. La máquina está en `install.rs`; acá sólo
    // viaja el gesto, así el teclado y el mando no pueden hacer cosas
    // distintas con el mismo botón.
    InstallMove(i32),
    InstallActivate,
    /// X: usar esta carpeta (modo carpeta).
    InstallAction,
    /// Y: volver a la versión anterior, desde el resultado.
    InstallRevert,
    InstallClose,
    // El diálogo de controles del juego. La máquina está en `controls.rs`.
    ControlsMove(i32),
    /// A: capturar la tecla de la fila.
    ControlsKey,
    /// X: capturar el botón de la fila.
    ControlsButton,
    /// Y: todo a fábrica.
    ControlsRestore,
    ControlsClose,
    // #604 — la ficha y su historial
    PlayFromDetails,
    InstallFromDetails,
    NextPack,
    UninstallPack,
    ConfirmAccept,
    ConfirmCancel,
    NewGame,
    SaveRowAt(i32),
    RestoreSave(i32),
    CloseSaves,
    // #601 — crash y sesión huérfana
    SafeMode,
    CloseCrash,
    RecoverSession,
    DiscardSession,
    // Onboarding (#588)
    OnboardingRowAt(i32),
    /// A: hace lo del paso (marcar, elegir, entrar en una carpeta).
    OnboardingActivate,
    /// Continuar / Empezar.
    OnboardingNext,
    /// Atrás: paso anterior, o cerrar el explorador. Nunca cierra Play.
    OnboardingBack,
    /// Izquierda/derecha sobre la fila: la otra opción (hoy, el idioma).
    OnboardingSecondary(i32),
    /// #627 — la ficha: alternar entre el original y el pack HD.
    ToggleHD,
    /// El botón secundario del paso — «Quitar» una carpeta. Va en Y (norte),
    /// que es el botón que esta app ya reserva para «la acción secundaria de
    /// la pantalla». Sin él, quitar una carpeta sólo se podía con el mouse: un
    /// wizard gamepad-first con una acción que exige teclado y mouse.
    OnboardingStepSecondary,
    /// #627 — la ACCIÓN del paso: agregar otra carpeta, elegir otro core. Va en
    /// X porque el diseño nuevo pone «A Continuar» en el pie de las seis
    /// pantallas, y lo que A hacía antes necesitaba un botón donde vivir.
    OnboardingStepAction,
}

/// La superficie activa. El orden es la precedencia: un modal encima de otro
/// gana el que está más arriba en la pila visual.
pub fn surface(e: &State) -> Surface {
    use Surface::*;
    if e.playing {
        return Playing;
    }
    if e.confirm_open {
        return Confirmation;
    }
    // El wizard tapa todo lo demás: mientras está, no hay biblioteca detrás.
    if e.onboarding {
        return if e.browsing { Explorer } else { Onboarding };
    }
    // #601: el crash tapa la encuesta, por lo que dice su comentario en la
    // enum. Y las dos tapan todo lo de abajo: son lo último que pasó.
    if e.crash_open {
        return Crash;
    }
    if e.orphan_open {
        return OrphanSession;
    }
    if e.survey_open {
        return Survey;
    }
    // Créditos y ficha se abren SOBRE la configuración previa, así que van
    // antes que ella.
    if e.credits_open {
        return Credits;
    }
    // #604: los guardados se abren SOBRE la ficha, así que van antes.
    if e.saves_open {
        return Saves;
    }
    // #678: el diálogo de packs también — se abre desde Ajustes o desde la
    // ficha, y mientras está, es lo único que se toca.
    if e.install_open {
        return InstallPack;
    }
    if e.controls_open {
        return Controls;
    }
    if e.details_open {
        return GameDetails;
    }
    if e.config_open {
        return PreLaunch;
    }
    // LAS TRES SECCIONES DE CE: 0 Inicio · 1 Biblioteca · 2 Ajustes.
    //
    // AYTHER Play tiene seis: entre Biblioteca y Ajustes van Hub, Descargas y
    // Perfil, atenuadas hasta que haya cuenta. Acá no están —ni atenuadas—
    // porque en esta edición no van a llegar nunca (ADR-005), y una pestaña
    // apagada para siempre manda a buscar cómo encenderla.
    match e.tab {
        1 => Library,
        2 => Settings,
        _ => Home,
    }
}

/// Mueve un cursor dentro de `[0, n)` sin salirse ni envolver.
///
/// Sin envolver a propósito: en una lista de sala, que el cursor salte del
/// último al primero cuando alguien mantiene abajo hace perder el lugar. El
/// tope se siente, y eso es información.
/// El salto de una FILA en la grilla: `delta` filas hacia arriba o hacia abajo.
///
/// A diferencia de `mover`, esto **no clampea**: o la tarjeta de la fila destino
/// existe, o no se mueve. Clampear haría que abajo, en la última fila incompleta,
/// el cursor se corriera de columna para caer en el último juego — y el teclado,
/// que aplica esta misma regla en el `.slint`, no lo hace. Dos entradas que se
/// mueven distinto sobre la misma grilla es exactamente lo que #586 prohíbe.
fn move_grid_row(current: i32, delta: i32, columns: i32, n: i32) -> Option<i32> {
    let destination = current + delta * columns.max(1);
    (destination >= 0 && destination < n && destination != current).then_some(destination)
}

fn move_by(current: i32, delta: i32, n: i32) -> Option<i32> {
    if n <= 0 {
        return None;
    }
    let destination = (current + delta).clamp(0, n - 1);
    (destination != current).then_some(destination)
}

/// El ruteo. Una acción del mando, en el estado dado, produce un efecto.
pub fn route(e: &State, a: PadAction) -> Effect {
    use Effect as F;
    use PadAction::*;
    use Surface::*;

    match surface(e) {
        // El juego tiene la pantalla y el mando: Play no compite por ellos.
        Playing => F::NoOp,

        // Una lista larga de carpetas y archivos: arriba/abajo mueve, A entra
        // o elige, B cierra el explorador y devuelve al paso.
        Explorer => {
            match a {
                Up => move_by(e.onboarding_row, -1, e.onboarding_rows)
                    .map_or(F::NoOp, F::OnboardingRowAt),
                Down => move_by(e.onboarding_row, 1, e.onboarding_rows)
                    .map_or(F::NoOp, F::OnboardingRowAt),
                Accept => F::OnboardingActivate,
                // X elige la carpeta actual sin tener que buscar una fila «elegir
                // ésta»: es el atajo del que ya llegó a donde quería, y es lo que
                // el pie anuncia («X Usar esta carpeta»). Va por el MISMO efecto
                // que la acción del paso —`Estado::accion_del_paso()` sabe que
                // explorando eso significa elegir la carpeta—, así el teclado y el
                // mando no pueden separarse. Start hace lo mismo por costumbre, sin
                // anunciarse.
                SecondaryAction => F::OnboardingStepAction,
                Start => F::OnboardingNext,
                Back => F::OnboardingBack,
                Left | Right | Menu | PrevTab | NextTab => F::NoOp,
            }
        }

        Onboarding => {
            match a {
                Up => move_by(e.onboarding_row, -1, e.onboarding_rows)
                    .map_or(F::NoOp, F::OnboardingRowAt),
                Down => move_by(e.onboarding_row, 1, e.onboarding_rows)
                    .map_or(F::NoOp, F::OnboardingRowAt),
                // #627 — A CONTINÚA, que es lo que el pie del diseño dice en las
                // seis pantallas. `Estado::activar()` se desvía solo cuando el paso
                // no tiene con qué continuar (sin carpetas, sin core, sin aceptar):
                // ahí hace lo único que destraba, y su etiqueta lo anuncia.
                //
                // Antes A actuaba y Start continuaba, separados para que un botón
                // no confirmara sin querer. Con A anunciado paso por paso el riesgo
                // se va: lo que A hace está escrito en el pie, en esa pantalla.
                Accept | Start => F::OnboardingActivate,
                // X: la acción del paso —agregar otra carpeta, elegir otro core—,
                // que es donde el diseño la pone.
                SecondaryAction => F::OnboardingStepAction,
                // Izquierda/derecha rotan la opción de la fila (el idioma).
                Left => F::OnboardingSecondary(-1),
                Right => F::OnboardingSecondary(1),
                Back => F::OnboardingBack,
                // Y saca la carpeta enfocada. Sigue siendo el botón de la acción
                // destructiva y por eso no comparte tecla con «agregar».
                Menu => F::OnboardingStepSecondary,
                PrevTab | NextTab => F::NoOp,
            }
        }

        Survey => match a {
            Up => move_by(e.survey_row, -1, e.survey_row_count).map_or(F::NoOp, F::SurveyRow),
            Down => move_by(e.survey_row, 1, e.survey_row_count).map_or(F::NoOp, F::SurveyRow),
            Left => F::Rate(e.survey_row, -1),
            Right => F::Rate(e.survey_row, 1),
            // A puntúa (+1), Start guarda: el mismo criterio que en la
            // configuración previa — un botón que a veces ajusta y a veces
            // confirma es el que hace que alguien mande algo sin querer.
            Accept => F::Rate(e.survey_row, 1),
            Start => F::SaveSurvey,
            Back => F::SurveyLater,
            Menu | SecondaryAction | PrevTab | NextTab => F::NoOp,
        },

        // #601 — un camino y una salida. A reintenta en modo seguro; B cierra
        // sin hacer nada. No hay cursor porque no hay lista: dos botones con
        // efectos opuestos, y el destructivo no está.
        Crash => match a {
            Accept | Start => F::SafeMode,
            Back => F::CloseCrash,
            _ => F::NoOp,
        },

        // #601 — A recupera y B **no descarta**: descartar borra algo y B es la
        // tecla que la gente aprieta para salir de donde no quería entrar.
        // Descartar se elige a propósito, con Menu o con el mouse.
        OrphanSession => match a {
            Accept | Start => F::RecoverSession,
            Menu => F::DiscardSession,
            _ => F::NoOp,
        },

        Credits => match a {
            Back | Accept => F::CloseCredits,
            _ => F::NoOp,
        },

        // #604 — la ficha deja de ser sólo «cerrar».
        //
        // Sus tres acciones estaban únicamente en el mouse: con el mando,
        // A cerraba la ficha en vez de jugar, así que llegar a la ficha y
        // querer jugar obligaba a salir de ella primero. Ahora A juega —o
        // continúa, según haya guardado— Y empieza una partida nueva, X
        // cambia HD/original, G abre el historial y B cierra.
        GameDetails => match a {
            Up => F::InstallFromDetails,
            Right => F::NextPack,
            Down => F::UninstallPack,
            Accept | Start => F::PlayFromDetails,
            Menu => F::NewGame,
            // #627 — X alterna Original/HD, que es lo que el pie de la ficha
            // anuncia (y lo que el teclado ya hacia con la tecla `x`). Sin esta
            // linea el pie prometia un boton que el mando no ruteaba.
            SecondaryAction => F::ToggleHD,
            Back => F::CloseDetails,
            _ => F::NoOp,
        },

        Confirmation => match a {
            Start => F::ConfirmAccept,
            Back => F::ConfirmCancel,
            _ => F::NoOp,
        },

        // #604 — el historial. Arriba/abajo elige y A restaura; la fila 0 es la
        // partida en curso y restaurarla no haría nada, así que no se ofrece.
        // El diálogo de controles del juego. A captura la tecla de la fila,
        // X el botón, Y restaura la fábrica y B cierra — o cancela la captura,
        // que `controls::Dialogo` resuelve en dos escalones. El botón que se
        // está capturando NO pasa por acá: lo come el hilo del mando.
        Controls => match a {
            Up => F::ControlsMove(-1),
            Down => F::ControlsMove(1),
            Accept => F::ControlsKey,
            SecondaryAction => F::ControlsButton,
            Menu => F::ControlsRestore,
            Back => F::ControlsClose,
            _ => F::NoOp,
        },

        // #678 — el diálogo de packs. Como el explorador del wizard: la
        // cruceta navega, A entra o elige, X designa la carpeta que se está
        // mirando, Y revierte desde el resultado y B sale. Qué significa cada
        // gesto según la fase lo decide `install.rs`, que se prueba sin
        // ventana — acá no hay un solo `if`.
        InstallPack => match a {
            Up => F::InstallMove(-1),
            Down => F::InstallMove(1),
            Accept => F::InstallActivate,
            SecondaryAction => F::InstallAction,
            Menu => F::InstallRevert,
            Back => F::InstallClose,
            _ => F::NoOp,
        },

        Saves => match a {
            Up => move_by(e.save_row, -1, e.saves)
                .map(F::SaveRowAt)
                .unwrap_or(F::NoOp),
            Down => move_by(e.save_row, 1, e.saves)
                .map(F::SaveRowAt)
                .unwrap_or(F::NoOp),
            Accept | Start => {
                if e.save_row > 0 {
                    F::RestoreSave(e.save_row)
                } else {
                    F::NoOp
                }
            }
            Back => F::CloseSaves,
            _ => F::NoOp,
        },

        PreLaunch => match a {
            Up => move_by(e.config_row, -1, e.config_row_count).map_or(F::NoOp, F::ConfigRowAt),
            Down => move_by(e.config_row, 1, e.config_row_count).map_or(F::NoOp, F::ConfigRowAt),
            Left => F::AdjustConfig(e.config_row, -1),
            Right | Accept => F::AdjustConfig(e.config_row, 1),
            Start => F::AcceptConfig,
            Back => F::CancelConfig,
            Menu | SecondaryAction | PrevTab | NextTab => F::NoOp,
        },

        // #594 — Inicio: A continúa el último jugado. Arriba/abajo NO mueven
        // nada: el hero es uno solo y la fila de recientes se recorre con
        // izquierda/derecha… salvo que ésas cambian de sección, que es el gesto
        // del launcher. Se resuelve dejando que Inicio sólo continúe: entrar a
        // la fila de recientes es navegación de #605, cuando el chasis tenga las
        // seis secciones y el foco pueda bajar de la TopBar al contenido.
        Home => match a {
            Accept | Start => {
                if e.games > 0 {
                    F::Launch(e.game)
                } else {
                    F::NoOp
                }
            }
            // Desde la raíz, izquierda no va a ninguna parte y derecha entra a
            // la biblioteca. **Atrás nunca cierra Play** (§9).
            Right | NextTab => F::GoToTab(1),
            Left | PrevTab => F::NoOp,
            _ => F::NoOp,
        },
        // Ajustes es la última sección: a la derecha no hay nada. En CE la
        // anterior es la Biblioteca y no Perfil, porque las tres del Hub no
        // existen — el paso a la izquierda vale 1, no 4.
        Settings => match a {
            Up => move_by(e.setting_row, -1, e.settings_count).map_or(F::NoOp, F::SettingsRowAt),
            Down => move_by(e.setting_row, 1, e.settings_count).map_or(F::NoOp, F::SettingsRowAt),
            Accept | Start => F::ActivateSetting(e.setting_row),
            // #678 — Y: la secundaria de la fila. En las filas sin una,
            // `settings::secundaria` devuelve None y el callback no hace nada:
            // la decisión vive con las filas, no acá.
            Menu => F::SettingSecondary(e.setting_row),
            Left | PrevTab => F::GoToTab(1),
            Right | NextTab => F::NoOp,
            Back => F::GoToTab(1),
            _ => F::NoOp,
        },

        // #629 — LA GRILLA SE RECORRE CON LA CRUCETA; DE SECCIÓN SE CAMBIA CON
        // LB/RB.
        //
        // Antes ←→ cambiaban de sección, así que la cruceta sólo podía subir y
        // bajar: con seis columnas, cinco de cada seis tarjetas eran
        // inalcanzables con el mando —el cursor bajaba en línea recta y se
        // salteaba las otras cinco— y el único intento natural de alcanzarlas,
        // empujar a la derecha, sacaba al jugador al Hub.
        //
        // Ahora ←→ son ±1 sobre la biblioteca entera: la derecha de la última
        // columna es la primera de la fila de abajo, y en el último juego no
        // pasa nada. Arriba/abajo saltan una FILA, que es lo que una grilla
        // significa. Es la misma regla que el teclado aplica en el `.slint`.
        Library => match a {
            Up => move_grid_row(e.game, -1, e.columns, e.games).map_or(F::NoOp, F::SelectGame),
            Down => move_grid_row(e.game, 1, e.columns, e.games).map_or(F::NoOp, F::SelectGame),
            Left => move_by(e.game, -1, e.games).map_or(F::NoOp, F::SelectGame),
            Right => move_by(e.game, 1, e.games).map_or(F::NoOp, F::SelectGame),
            Accept | Start => {
                if e.games > 0 {
                    F::Launch(e.game)
                } else {
                    F::NoOp
                }
            }
            // Las tres secciones de CE: 0=Inicio · 1=Biblioteca · 2=Ajustes.
            PrevTab => F::GoToTab(0),
            NextTab => F::GoToTab(2),
            // B no hace NADA, y nunca cierra Play (§9). En AYTHER Play descarta
            // el aviso de actualización; acá no hay aviso que descartar.
            Back => F::NoOp,
            Menu | SecondaryAction => F::NoOp,
        },
    }
}

/// Los botones que la superficie activa usa, para el pie de pantalla. El texto
/// se traduce en la UI; acá va la clave, que es lo que no puede divergir del
/// ruteo de arriba.
pub fn hints(e: &State) -> &'static str {
    use Surface::*;
    match surface(e) {
        Playing => "jugando",
        Confirmation => "confirmar",
        Explorer => "explorador",
        Onboarding => "onboarding",
        Crash => "crash",
        OrphanSession => "huerfana",
        Survey => "encuesta",
        Credits => "creditos",
        Saves => "guardados",
        InstallPack => "instalar",
        Controls => "controles",
        GameDetails => "ficha",
        PreLaunch => "config",
        // Las cinco secciones no se tapan entre sí —son excluyentes por
        // pestaña— así que se listan por índice DESCENDENTE, que es el orden en
        // que la cadena de la UI las pregunta. Cualquier otro orden acá dejaría
        // a `pad_focus.rs` comparando dos listas que dicen lo mismo con
        // palabras distintas.
        Settings => "ajustes",
        Library => "biblioteca",
        Home => "inicio",
    }
}

#[cfg(test)]
mod tests {
    use super::Effect as F;
    use super::*;
    use crate::gamepad::PadAction::*;

    #[test]
    fn pack_management_and_confirmation_are_reachable_with_a_controller() {
        let mut state = State {
            details_open: true,
            ..Default::default()
        };
        assert_eq!(route(&state, Up), F::InstallFromDetails);
        assert_eq!(route(&state, Right), F::NextPack);
        assert_eq!(route(&state, Down), F::UninstallPack);
        state.confirm_open = true;
        assert_eq!(route(&state, Accept), F::NoOp);
        assert_eq!(route(&state, Start), F::ConfirmAccept);
        assert_eq!(route(&state, Back), F::ConfirmCancel);
        assert_eq!(route(&state, Down), F::NoOp);
    }

    /// #594: la Biblioteca pasó a ser la sección **1** — Inicio es la 0. El
    /// helper lo dice explícito para que ningún test dependa del default.
    fn library() -> State {
        State {
            tab: 1,
            games: 5,
            game: 2,
            ..Default::default()
        }
    }

    fn home() -> State {
        State {
            tab: 0,
            games: 5,
            game: 2,
            ..Default::default()
        }
    }

    /// #601 — el crash tapa la encuesta. Las dos aparecen al volver de una
    /// sesión, y preguntarle a alguien qué le pareció el pack justo después de
    /// que se le cayó el juego es preguntar por el crash.
    /// #604 — EL DEFECTO QUE ESTO CIERRA: en la ficha, A cerraba la ficha en
    /// vez de jugar. Llegar a la ficha de un juego y querer jugarlo obligaba a
    /// salir de ella primero, y las tres acciones que la ficha ofrece estaban
    /// únicamente en el mouse — en un launcher de 10 pies.
    #[test]
    fn on_the_card_accept_plays_instead_of_closing_it() {
        let e = State {
            details_open: true,
            ..Default::default()
        };
        assert_eq!(route(&e, Accept), F::PlayFromDetails);
        assert_eq!(route(&e, Menu), F::NewGame);
        assert_eq!(route(&e, Back), F::CloseDetails, "y Atras sigue saliendo");
    }

    /// El historial se abre SOBRE la ficha, así que la tapa: con los dos
    /// abiertos, las flechas mueven la lista de guardados y no la ficha.
    #[test]
    fn the_saves_cover_the_card() {
        let e = State {
            details_open: true,
            saves_open: true,
            saves: 3,
            save_row: 0,
            ..Default::default()
        };
        assert_eq!(surface(&e), Surface::Saves);
        assert_eq!(route(&e, Down), F::SaveRowAt(1));
    }

    /// Restaurar la fila 0 —la partida en curso— no hace nada: duplicaría lo
    /// que ya se está jugando y la lista diría dos veces lo mismo.
    #[test]
    fn restoring_the_ongoing_play_does_nothing() {
        let base = State {
            saves_open: true,
            saves: 3,
            ..Default::default()
        };
        assert_eq!(
            route(
                &State {
                    save_row: 0,
                    ..base
                },
                Accept
            ),
            F::NoOp
        );
        assert_eq!(
            route(
                &State {
                    save_row: 2,
                    ..base
                },
                Accept
            ),
            F::RestoreSave(2)
        );
    }

    #[test]
    fn the_crash_covers_the_survey() {
        let e = State {
            crash_open: true,
            survey_open: true,
            ..Default::default()
        };
        assert_eq!(surface(&e), Surface::Crash);
    }

    /// A reintenta en modo seguro y B cierra. Es el único camino: volver a
    /// apretar A sobre el juego relanzaría lo mismo y volvería a fallar.
    #[test]
    fn the_crash_offers_safe_mode_on_a_and_exit_on_b() {
        let e = State {
            crash_open: true,
            ..Default::default()
        };
        assert_eq!(route(&e, Accept), F::SafeMode);
        assert_eq!(route(&e, Start), F::SafeMode);
        assert_eq!(route(&e, Back), F::CloseCrash);
    }

    /// EL DEFECTO QUE ESTO EVITA: en la sesión huérfana, B **no descarta**.
    /// Descartar borra algo, y B es la tecla que uno aprieta para salir de
    /// donde no quería entrar — que es exactamente cómo se pierde una partida
    /// sin haberlo decidido.
    #[test]
    fn on_the_orphan_session_back_discards_nothing() {
        let e = State {
            orphan_open: true,
            ..Default::default()
        };
        assert_eq!(route(&e, Accept), F::RecoverSession);
        assert_eq!(route(&e, Menu), F::DiscardSession);
        assert_eq!(
            route(&e, Back),
            F::NoOp,
            "Atras no puede borrar la sesion de nadie"
        );
        assert_ne!(route(&e, Left), F::DiscardSession);
        assert_ne!(route(&e, Right), F::DiscardSession);
    }

    /// Y el wizard sigue tapándola: si el wizard está, no hubo biblioteca de la
    /// cual venir, así que no puede haber una sesión anterior que recuperar.
    #[test]
    fn the_wizard_covers_the_orphan_session() {
        let e = State {
            onboarding: true,
            orphan_open: true,
            ..Default::default()
        };
        assert_eq!(surface(&e), Surface::Onboarding);
    }

    #[test]
    fn surface_precedence_is_the_visual_stack() {
        // Todos los flags a la vez: gana el de más arriba. Es el caso que el
        // `if` en cascada resolvía por orden de escritura, sin que nadie lo
        // hubiera decidido.
        let e = State {
            // `tab: 2` para que al destapar los modales aparezca Ajustes; con
            // el default (0) aparecería Inicio y el test no probaría la parte
            // de abajo de la pila.
            tab: 2,
            playing: true,
            survey_open: true,
            credits_open: true,
            details_open: true,
            config_open: true,
            ..Default::default()
        };
        assert_eq!(surface(&e), Surface::Playing);
        let e = State {
            playing: false,
            ..e
        };
        assert_eq!(surface(&e), Surface::Survey);
        let e = State {
            survey_open: false,
            ..e
        };
        assert_eq!(surface(&e), Surface::Credits);
        let e = State {
            credits_open: false,
            ..e
        };
        assert_eq!(surface(&e), Surface::GameDetails);
        let e = State {
            details_open: false,
            ..e
        };
        assert_eq!(surface(&e), Surface::PreLaunch);
        let e = State {
            config_open: false,
            ..e
        };
        assert_eq!(surface(&e), Surface::Settings);
        let e = State { tab: 1, ..e };
        assert_eq!(surface(&e), Surface::Library);
        // #594: debajo de todo, Inicio — la sección 0 y la que Play abre.
        let e = State { tab: 0, ..e };
        assert_eq!(surface(&e), Surface::Home);
    }

    /// #678 — el diálogo de packs tapa lo que tenga debajo: se abre desde
    /// Ajustes o desde la ficha, y mientras navega el disco nada de atrás se
    /// mueve. Sólo los guardados —que también son un modal sobre la ficha— y
    /// lo más urgente (crash, huérfana) le ganan.
    #[test]
    fn the_packs_dialog_covers_the_card_and_settings() {
        let e = State {
            install_open: true,
            details_open: true,
            tab: 2,
            ..Default::default()
        };
        assert_eq!(surface(&e), Surface::InstallPack);
        let e = State {
            saves_open: true,
            ..e
        };
        assert_eq!(
            surface(&e),
            Surface::Saves,
            "los guardados van encima: son otro modal de la ficha"
        );
        let e = State {
            crash_open: true,
            ..e
        };
        assert_eq!(surface(&e), Surface::Crash);
    }

    /// #594 — en Inicio, A continúa el último jugado.
    ///
    /// Y sin biblioteca no hace NADA: lanzar el índice 0 de una lista vacía es
    /// como se cuela un «apretó A y no pasó nada, o peor, arrancó cualquiera».
    #[test]
    fn on_home_accept_continues_the_latest() {
        assert_eq!(route(&home(), Accept), F::Launch(2));
        assert_eq!(route(&home(), Start), F::Launch(2));
        let empty_state = State { games: 0, ..home() };
        assert_eq!(
            route(&empty_state, Accept),
            F::NoOp,
            "sin juegos no se lanza nada"
        );
    }

    #[test]
    fn back_never_closes_the_application() {
        // La regla de §9, superficie por superficie: B siempre sube un nivel o
        // no hace nada, y NUNCA produce un efecto que termine la app.
        let cases = [
            (State { ..home() }, F::NoOp),    // raíz de verdad
            (State { ..library() }, F::NoOp), // Biblioteca
            (
                State {
                    tab: 2,
                    ..library()
                },
                F::GoToTab(1),
            ), // Ajustes → Biblioteca
            (
                State {
                    config_open: true,
                    ..library()
                },
                F::CancelConfig,
            ),
            (
                State {
                    config_open: true,
                    details_open: true,
                    ..library()
                },
                F::CloseDetails,
            ),
            (
                State {
                    config_open: true,
                    credits_open: true,
                    ..library()
                },
                F::CloseCredits,
            ),
            (
                State {
                    survey_open: true,
                    ..library()
                },
                F::SurveyLater,
            ),
        ];
        for (e, expected) in cases {
            assert_eq!(route(&e, Back), expected, "Atrás en {:?}", surface(&e));
        }
    }

    #[test]
    fn every_surface_responds_to_the_gamepad() {
        // El defecto que #586 arregla: cinco de siete superficies ignoraban el
        // mando. Ninguna puede quedarse muda ante A, ni ante arriba/abajo si
        // tiene filas.
        let active_surfaces = [
            State {
                survey_open: true,
                survey_row_count: 7,
                ..library()
            },
            State {
                config_open: true,
                credits_open: true,
                ..library()
            },
            State {
                config_open: true,
                details_open: true,
                ..library()
            },
            State {
                config_open: true,
                config_row_count: 4,
                ..library()
            },
            State {
                tab: 2,
                settings_count: 9,
                ..library()
            },
            State {
                install_open: true,
                ..library()
            },
            State {
                onboarding: true,
                onboarding_rows: 2,
                ..library()
            },
            State {
                onboarding: true,
                browsing: true,
                onboarding_rows: 9,
                ..library()
            },
            library(),
        ];
        for e in active_surfaces {
            assert_ne!(
                route(&e, Accept),
                F::NoOp,
                "«A» no hace nada en {:?}",
                surface(&e)
            );
        }
        // Y la única que calla es la que debe: durante el juego, el mando es
        // del Runtime.
        let playing = State {
            playing: true,
            ..library()
        };
        for a in [
            Up, Down, Left, Right, Accept, Back, Start, Menu, PrevTab, NextTab,
        ] {
            assert_eq!(route(&playing, a), F::NoOp);
        }
    }

    /// El wizard tapa todo: mientras corre, la biblioteca no existe. Si el mando siguiera moviendo la lista de atras, el jugador
    /// veria el foco saltar en una pantalla que no esta mirando.
    #[test]
    fn onboarding_wins_over_everything_but_the_game() {
        let e = State {
            onboarding: true,
            survey_open: true,
            details_open: true,
            config_open: true,
            tab: 1,
            ..library()
        };
        assert_eq!(surface(&e), Surface::Onboarding);
        // Y el explorador, encima del wizard.
        let e = State {
            browsing: true,
            ..e
        };
        assert_eq!(surface(&e), Surface::Explorer);
        // Pero el juego sigue ganando: si el Runtime tiene la pantalla, Play
        // no escucha nada, ni siquiera para el wizard.
        let e = State { playing: true, ..e };
        assert_eq!(surface(&e), Surface::Playing);
    }

    /// La regla del chasis dentro del wizard: Atras retrocede, y en el primer
    /// paso `onboarding::Estado::retroceder` decide no hacer nada. Lo que NO
    /// puede pasar es que el router mande cerrar.
    #[test]
    fn back_in_onboarding_only_steps_back() {
        let e = State {
            onboarding: true,
            onboarding_rows: 2,
            ..library()
        };
        assert_eq!(route(&e, Back), F::OnboardingBack);
        let e = State {
            browsing: true,
            ..e
        };
        assert_eq!(route(&e, Back), F::OnboardingBack);
    }

    /// #627 — A y Start van al mismo lado, y ese lado es el paso.
    ///
    /// Antes A actuaba y Start continuaba, separados para que marcar el
    /// checkbox de los terminos no avanzara en el mismo gesto —aceptar sin
    /// leer—. El disenio nuevo pone «A Continuar» en el pie de las seis
    /// pantallas, asi que A entra por `Estado::activar()`, que es quien decide
    /// si eso significa continuar. La proteccion no se fue de lugar: sigue
    /// probada en `onboarding::tests::la_etiqueta_de_a_dice_lo_que_a_hace`, que
    /// exige que en Terminos sin aceptar A ACEPTE y no avance.
    #[test]
    fn a_enters_through_the_step_and_not_a_shortcut() {
        let e = State {
            onboarding: true,
            onboarding_rows: 3,
            ..library()
        };
        assert_eq!(route(&e, Accept), F::OnboardingActivate);
        assert_eq!(route(&e, Start), F::OnboardingActivate);
    }

    /// #627 — X es la accion del paso: agregar otra carpeta, elegir otro core.
    /// Sin un boton propio, con A ocupado en continuar esa accion se quedaba
    /// sin mando.
    #[test]
    fn the_step_action_is_on_x() {
        let e = State {
            onboarding: true,
            onboarding_rows: 2,
            ..library()
        };
        assert_eq!(route(&e, SecondaryAction), F::OnboardingStepAction);
        // En el explorador, X va por el mismo efecto: alli la accion del paso
        // es elegir la carpeta donde uno esta parado.
        let expl = State {
            browsing: true,
            ..e
        };
        assert_eq!(route(&expl, SecondaryAction), F::OnboardingStepAction);
        // Y en el resto del chasis no hace nada: es un boton del wizard.
        assert_eq!(route(&library(), SecondaryAction), F::NoOp);
    }

    /// Todo lo que el wizard puede hacer, se puede hacer con el mando. El
    /// unico agujero que tuvo fue el boton secundario del paso —quitar una
    /// carpeta—: estaba en pantalla, lo llamaba el mouse y ningun boton del
    /// mando lo alcanzaba. En un wizard gamepad-first eso no es un detalle:
    /// quien se equivoco de carpeta quedaba obligado a buscar un mouse.
    #[test]
    fn the_step_secondary_button_is_reachable_with_the_gamepad() {
        let e = State {
            onboarding: true,
            onboarding_rows: 2,
            ..library()
        };
        assert_eq!(route(&e, Menu), F::OnboardingStepSecondary);
        // En el explorador no hay secundario: la lista es una sola cosa.
        let expl = State {
            browsing: true,
            ..e
        };
        assert_eq!(route(&expl, Menu), F::NoOp);
    }

    /// En el explorador, izquierda y derecha no hacen nada: la lista es
    /// vertical. En el paso, en cambio, rotan la opcion de la fila (el idioma).
    #[test]
    fn horizontal_arrows_only_work_where_there_are_two_options() {
        let expl = State {
            onboarding: true,
            browsing: true,
            onboarding_rows: 5,
            ..library()
        };
        assert_eq!(route(&expl, Left), F::NoOp);
        assert_eq!(route(&expl, Right), F::NoOp);
        let step = State {
            browsing: false,
            ..expl
        };
        assert_eq!(route(&step, Left), F::OnboardingSecondary(-1));
        assert_eq!(route(&step, Right), F::OnboardingSecondary(1));
    }

    #[test]
    fn the_cursor_neither_escapes_nor_wraps() {
        // #594: en la BIBLIOTECA (tab 1). En Inicio el cursor no se mueve —
        // hay un hero y una fila, y recorrer la fila es navegación de #605.
        let e = State {
            game: 0,
            games: 3,
            ..library()
        };
        assert_eq!(route(&e, Up), F::NoOp, "arriba en el tope no envuelve");
        assert_eq!(route(&e, Down), F::SelectGame(1));
        let e = State { game: 2, ..e };
        assert_eq!(route(&e, Down), F::NoOp, "abajo en el fondo no envuelve");
        // Lista vacía: nada que elegir y nada que lanzar (el caso que crashea
        // un índice si nadie lo mira).
        let empty_state = State {
            game: 0,
            games: 0,
            ..library()
        };
        assert_eq!(route(&empty_state, Down), F::NoOp);
        assert_eq!(route(&empty_state, Accept), F::NoOp);
    }

    #[test]
    fn a_adjusts_and_start_confirms() {
        // La distinción que evita empezar una partida sin querer, tanto en la
        // configuración previa como en la encuesta.
        let cfg = State {
            config_open: true,
            config_row_count: 3,
            config_row: 1,
            ..library()
        };
        assert_eq!(route(&cfg, Accept), F::AdjustConfig(1, 1));
        assert_eq!(route(&cfg, Start), F::AcceptConfig);
        let enc = State {
            survey_open: true,
            survey_row_count: 7,
            survey_row: 2,
            ..library()
        };
        assert_eq!(route(&enc, Accept), F::Rate(2, 1));
        assert_eq!(route(&enc, Start), F::SaveSurvey);
    }

    /// EL ORÁCULO DE LA EDICIÓN, del lado del mando: en la raíz, B y Menu no
    /// hacen NADA.
    ///
    /// En AYTHER Play esos dos botones atienden el aviso de actualización —Y
    /// aplica, B descarta— desde Inicio y desde la Biblioteca. CE no se
    /// actualiza sola (ADR-005), así que el aviso no existe; lo que este test
    /// fija es que los botones queden MUDOS y no que hereden otra cosa. Un B
    /// que en la raíz hace algo es a un paso de ser un B que cierra Play, que
    /// es lo único que la §9 prohíbe sin excepciones.
    #[test]
    fn at_the_root_back_and_menu_do_nothing() {
        for e in [home(), library()] {
            assert_eq!(route(&e, Menu), F::NoOp, "Menu en {:?}", surface(&e));
            assert_eq!(route(&e, Back), F::NoOp, "Atrás en {:?}", surface(&e));
        }
    }

    /// #629 — LA CRUCETA RECORRE LA GRILLA; DE SECCIÓN SE CAMBIA CON LB/RB.
    ///
    /// El defecto que cierra: con ←→ cableadas a las secciones, la cruceta sólo
    /// subía y bajaba. En una grilla de seis columnas eso deja **cinco de cada
    /// seis tarjetas inalcanzables** con el mando —el cursor baja en línea recta
    /// y se saltea las otras cinco—, y el único intento natural de alcanzarlas,
    /// empujar a la derecha, sacaba al jugador al Hub. Ninguna de las dos cosas
    /// falla: las dos se sienten rotas.
    #[test]
    fn in_the_grid_arrows_walk_the_games_and_do_not_switch_sections() {
        // Seis columnas, catorce juegos, parado en el 5 (última columna de la
        // primera fila).
        let e = State {
            game: 5,
            games: 14,
            columns: 6,
            ..library()
        };
        assert_eq!(
            route(&e, Right),
            F::SelectGame(6),
            "la derecha de la última columna es la PRIMERA de la fila de abajo"
        );
        assert_eq!(
            route(&e, Down),
            F::SelectGame(11),
            "abajo salta una fila entera"
        );
        assert_eq!(
            route(&e, Up),
            F::NoOp,
            "en la primera fila, arriba no envuelve"
        );

        // Y al revés: la izquierda de la primera columna es la última de la
        // fila de arriba.
        let e = State { game: 6, ..e };
        assert_eq!(route(&e, Left), F::SelectGame(5));

        // Los dos extremos no hacen NADA —ni se mueven ni cambian de sección—.
        // Un borde quieto dice «no hay más» sin sacar a nadie de donde estaba.
        let first = State { game: 0, ..e };
        assert_eq!(route(&first, Left), F::NoOp);
        let last = State { game: 13, ..e };
        assert_eq!(route(&last, Right), F::NoOp);
        assert_eq!(
            route(&last, Down),
            F::NoOp,
            "abajo desde la última fila incompleta no se corre de columna"
        );

        // NINGUNA flecha cambia de sección: para eso están los gatillos.
        for a in [Left, Right, Up, Down] {
            for j in [0, 5, 6, 13] {
                let e = State { game: j, ..e };
                assert!(
                    !matches!(route(&e, a), F::GoToTab(_)),
                    "la flecha {a:?} desde el juego {j} cambió de sección"
                );
            }
        }

        // En vista de LISTA la ventana manda `columnas: 1`, y ahí arriba/abajo
        // vuelven a ser de a uno, que es lo que una lista significa.
        let list_state = State {
            game: 5,
            games: 14,
            columns: 1,
            ..library()
        };
        assert_eq!(route(&list_state, Down), F::SelectGame(6));
        assert_eq!(route(&list_state, Up), F::SelectGame(4));
    }

    #[test]
    fn the_triggers_switch_sections_from_either_of_the_two() {
        // TRES secciones (0=Inicio · 1=Biblioteca · 2=Ajustes). Cada una sabe
        // cuál tiene al lado; desde Inicio, «anterior» no va a ninguna parte
        // —lo que hace que la raíz se sienta raíz— y desde Ajustes, «siguiente»
        // tampoco: son los dos extremos, y envolver haría perder el lugar a
        // quien mantenga el gatillo apretado.
        //
        // El recorrido es CONTINUO: no hay un hueco donde estaban las tres
        // secciones del Hub. Ese hueco es el defecto que este test mira — con
        // los saltos de AYTHER Play copiados tal cual, RB desde la Biblioteca
        // llevaría a una sección 3 que acá no existe y el chasis quedaría
        // mostrando Inicio con la pestaña de Ajustes marcada.
        for (tab, previous, next) in [
            (0, F::NoOp, F::GoToTab(1)),
            (1, F::GoToTab(0), F::GoToTab(2)),
            (2, F::GoToTab(1), F::NoOp),
        ] {
            let e = State { tab, ..library() };
            assert_eq!(route(&e, PrevTab), previous, "anterior desde {tab}");
            assert_eq!(route(&e, NextTab), next, "siguiente desde {tab}");
        }
        // Pero NO dentro de un modal: cambiar de pestaña con un diálogo abierto
        // dejaría la configuración a medias detrás de otra pantalla.
        let cfg = State {
            config_open: true,
            ..library()
        };
        assert_eq!(route(&cfg, NextTab), F::NoOp);
    }
}
