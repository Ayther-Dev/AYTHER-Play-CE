//! game_details.rs — de qué depende jugar, y de qué depende el pack (#597, #631, #632).
//!
//! # Dos preguntas, no tres semáforos (#632)
//!
//! La ficha mostraba ROM · core · pack como tres ejes hermanos, y de los tres
//! salía un veredicto. Eran dos preguntas disfrazadas de tres:
//!
//! - **¿esto corre?** — la contesta el core, y es la que decide si se juega;
//! - **¿esto está mejorado?** — la contesta el pack, y es opcional.
//!
//! El tercer eje nunca fue un tercero: `eje_rom` sólo lee códigos del validador
//! **del pack** y sin pack no dice nada. Presentado al lado del core se leía
//! como «tu ROM está mal», que es otra cosa y manda a arreglar lo que no está
//! roto. Sigue existiendo —un pack horneado para otra ROM arranca y sustituye
//! donde no va— pero como una razón del PACK, que es de lo que habla.
//!
//! Los cuatro casos que el validador distingue no se colapsan: «es de otro
//! juego», «no dice de cuál es», «no se pudo comparar» y «coincide» llevan a
//! acciones distintas. Lo que cambia es dónde se leen, no cuánto se sabe.
//!
//! # De dónde salen
//!
//! Del MISMO informe que produce el grado general (`ayther_core::pack_validate`,
//! #530) — no de una segunda evaluación. Si los ejes se calcularan aparte, la
//! ficha podría contradecir a la tarjeta sobre el mismo pack, y ahí el jugador
//! tiene dos diagnósticos y ninguna manera de saber cuál vale.
//!
//! El eje del CORE es el único que no sale del informe, porque el validador no
//! sabe qué cores tiene instalados el jugador: eso lo aporta la configuración
//! (#589/#592). Es exactamente el eje que faltaba antes de #595.

use crate::cores::CoreAxis;
use crate::library::PackStatus;

/// Un eje: su grado y por qué.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub state: PackStatus,
    pub reason: String,
}

impl Axis {
    fn new(state: PackStatus, reason: &str) -> Self {
        Axis {
            state,
            reason: reason.to_string(),
        }
    }
}

/// Los tres ejes de un juego.
#[derive(Debug, Clone, PartialEq)]
pub struct Axes {
    pub rom: Axis,
    pub core: Axis,
    pub pack: Axis,
}

/// El eje de la ROM, leído de los códigos del informe.
///
/// Los cuatro casos son los cuatro que el validador distingue, y cada uno dice
/// algo distinto: coincide · no coincide · el pack no declara contra qué se
/// horneó · nadie pudo comparar. Colapsarlos en «sí/no» perdería justamente la
/// diferencia entre «este pack es de otro juego» y «este pack no dice de cuál
/// es», que para el jugador son problemas opuestos.
fn rom_axis(codes: &[String], has_pack: bool) -> Axis {
    if !has_pack {
        return Axis::new(PackStatus::NoPack, "Se juega el original, sin pack.");
    }
    if codes.iter().any(|c| c == "rom.mismatch") {
        return Axis::new(
            PackStatus::Incompatible,
            "El pack se horneó para otra ROM. No es de este juego, o es de otra revisión.",
        );
    }
    if codes.iter().any(|c| c == "rom.unverified") {
        return Axis::new(
            PackStatus::Unverified,
            "El pack declara una ROM pero no se pudo comparar con la tuya.",
        );
    }
    if codes.iter().any(|c| c == "rom.undeclared") {
        return Axis::new(
            PackStatus::Warnings,
            "El pack no declara para qué ROM se horneó: si es de otra, se va a notar jugando.",
        );
    }
    Axis::new(PackStatus::Exact, "El pack se horneó para esta ROM exacta.")
}

/// El eje del CORE. No sale del informe: el validador no sabe qué tiene
/// instalado el jugador.
///
/// **Lo decide el SONDEO** (`cores::eje`, #642) y no que el archivo exista.
/// Hasta #642 salía de `hay_core_para`, que sólo comprobaba la existencia: un
/// core de 32 bits, uno corrupto o uno de otra consola daban «Compatible» acá y
/// el juego no arrancaba. Desde #631 este eje decide solo si se puede jugar, así
/// que tenía que pasar a valer lo que dice.
///
/// Los seis estados se traducen a tres grados, y la línea que importa es la del
/// medio: **«todavía no lo comprobé» y «no lo pude comprobar» NO son un no**.
/// Tratarlos como tal haría que Play dijera «falta el core» de toda la
/// biblioteca durante el segundo que tarda el sondeo del arranque, o que un
/// runtime que falta dejara al jugador sin jugar por un defecto nuestro.
fn core_axis(core: &CoreAxis, platform: &str) -> Axis {
    match core {
        CoreAxis::Missing => Axis {
            state: PackStatus::Incompatible,
            reason: format!(
                "No hay un core configurado que corra {platform}. \
                             Elegí uno en Ajustes."
            ),
        },
        // El texto viene del sondeo y ya está escrito para que el jugador pueda
        // actuar: «Descargá la versión x64 del core: la que elegiste es x86».
        CoreAxis::Unusable(reason) => Axis::new(PackStatus::Incompatible, reason),
        CoreAxis::Unchecked => Axis::new(
            PackStatus::Unverified,
            "Hay un core, pero todavía no se comprobó que sirva para esta consola.",
        ),
        CoreAxis::CheckFailed => Axis::new(
            PackStatus::Unverified,
            "Hay un core, pero Ayther no lo pudo comprobar. Revisá la instalación.",
        ),
        CoreAxis::Undeclared => Axis::new(
            PackStatus::Unverified,
            "El core no declara qué consolas corre, así que puede rechazar esta ROM.",
        ),
        CoreAxis::Usable => Axis {
            state: PackStatus::Exact,
            reason: format!("Hay un core que corre {platform}."),
        },
    }
}

/// Los tres ejes de una ficha.
///
/// `grado_pack` y `motivo_pack` vienen del veredicto general —el mismo que la
/// tarjeta— para que las dos superficies no puedan discrepar.
pub fn axes(
    codes: &[String],
    has_pack: bool,
    pack_grade: PackStatus,
    pack_reason: &str,
    core: &CoreAxis,
    platform: &str,
) -> Axes {
    Axes {
        rom: rom_axis(codes, has_pack),
        core: core_axis(core, platform),
        pack: Axis {
            state: pack_grade,
            reason: pack_reason.to_string(),
        },
    }
}

/// En qué situación está el pack de este juego (#633).
///
/// Cuatro, y cada uno lleva a una acción distinta. Antes esto no existía como
/// concepto: había un conmutador de HD que escribía el perfil de la próxima
/// sesión, y el jugador tenía que deducir del color del riel si el pack estaba
/// puesto. Pero lo que se elige no es «original o mejorado» dentro de un pack:
/// se elige **si el pack está puesto**, y eso tiene nombre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackState {
    /// No hay pack para este juego. No es un problema: se juega el original.
    None,
    /// Hay uno y está puesto.
    Active,
    /// Hay uno, se puede usar, y está apagado.
    Disabled,
    /// Hay uno y el validador lo rechazó, o se horneó para otra ROM.
    Unusable,
}

/// Lo único que la sección ofrece hacer (#633).
///
/// Una por estado, nunca dos. Una sección con dos botones obliga a elegir antes
/// de entender, y a tres metros eso es una pantalla que no se usa.
///
/// Vive para los oráculos: la UI trabaja con el índice del estado y no con este
/// enum, pero la tabla estado→acción es justamente lo que hay que poder probar
/// sin abrir una ventana. El `allow` no cubre `cargo test`: el día que el test
/// se vaya, el aviso vuelve.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackAction {
    /// Poner el pack. Escribe el perfil de la próxima sesión y nada más.
    Activate,
    /// Sacarlo y jugar el original. Reversible, no toca la ROM ni los guardados.
    Deactivate,
    /// #678 — abrir el diálogo de instalar un pack desde el disco. Es el
    /// «Conseguir» de CE: en AYTHER Play este estado lleva al Hub; acá el
    /// pack ya lo tiene el jugador y lo que falta es traerlo con el mando.
    Install,
    /// Ninguna: un pack que no sirve no se arregla apretando un botón. Lo
    /// único que la ficha puede hacer por él es decir por qué.
    NoOp,
}

impl PackState {
    /// La tabla entera, en un solo lugar y sin ventana de por medio.
    ///
    /// `activo` es lo que el perfil de la sesión tiene escrito hoy; puede decir
    /// que sí sobre un pack que no sirve —quedó de antes de que el pack se
    /// rompiera o se cambiara la ROM— y en ese caso manda el pack, no el perfil:
    /// mostrar «Activo» sobre algo que Play va a dejar afuera de la sesión es la
    /// ficha contradiciéndose sola.
    pub fn of(has_pack: bool, usable: bool, active: bool) -> Self {
        if !has_pack {
            return PackState::None;
        }
        if !usable {
            return PackState::Unusable;
        }
        if active {
            PackState::Active
        } else {
            PackState::Disabled
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn action(self) -> PackAction {
        match self {
            PackState::None => PackAction::Install,
            PackState::Active => PackAction::Deactivate,
            PackState::Disabled => PackAction::Activate,
            PackState::Unusable => PackAction::NoOp,
        }
    }

    /// Las PALABRAS no están acá: las pone la UI, que es donde vive el resto de
    /// los textos y por donde pasa el catálogo de traducción (`@tr`). Acá vive
    /// la máquina —qué estado hay y qué acción ofrece—, que es lo que se puede
    /// probar sin abrir una ventana. Que la UI tenga una etiqueta por estado lo
    /// fija su propio oráculo, en `tests/pad_focus.rs`.
    ///
    /// El índice que cruza a Slint, y con el que la UI decide qué decir y qué
    /// invocar.
    pub fn as_int(self) -> i32 {
        match self {
            PackState::None => 0,
            PackState::Active => 1,
            PackState::Disabled => 2,
            PackState::Unusable => 3,
        }
    }
}

impl Axes {
    /// El peor entre dos ejes, por el orden del enum —de «nada» a
    /// «incompatible»—, que es el mismo que usa el badge para elegir color.
    /// `SinPack` no cuenta: no tener pack no es un problema.
    fn worst_of(a: PackStatus, b: PackStatus) -> PackStatus {
        [a, b]
            .into_iter()
            .filter(|e| *e != PackStatus::NoPack)
            .max_by_key(|e| e.as_int())
            .unwrap_or(PackStatus::NoPack)
    }

    /// ¿Se puede jugar? **Lo decide el core y nada más** (#631).
    ///
    /// Antes salía de `peor()`, o sea de los tres ejes, y eso hacía que un pack
    /// roto —uno de otro juego, uno corrupto— dejara «Jugar» en gris sobre un
    /// juego que corre perfecto sin él. No había salida: apagar el conmutador de
    /// HD escribe el perfil de la próxima sesión y no recalcula el veredicto, y
    /// desde la grilla A abre la ficha, así que el juego quedaba inalcanzable
    /// por culpa de un archivo que sobra.
    ///
    /// Era además incoherente con lo que este mismo archivo ya decidía dos
    /// funciones más arriba: `SinPack` **no cuenta** para el veredicto, porque
    /// no tener pack no es un problema. Si no tenerlo no bloquea, tener uno malo
    /// tampoco puede — en los dos casos se juega exactamente lo mismo, el
    /// original.
    ///
    /// Lo que un pack roto sí bloquea es a sí mismo: ver `pack_usable`.
    pub fn is_playable(&self) -> bool {
        self.core.state.is_playable()
    }

    /// ¿Se puede jugar CON el pack? (#631)
    ///
    /// Es la otra mitad del veredicto viejo, y la que sigue mereciendo mirar la
    /// ROM: un pack horneado para otra ROM se instala y arranca, pero sustituye
    /// donde no va. El eje de la ROM habla del pack —`eje_rom` sólo lee códigos
    /// del validador del pack— así que su lugar natural es éste y no el de al
    /// lado del core (#632 lo muda también en la pantalla).
    ///
    /// Sin pack es `false` sin que eso sea un problema: no hay nada que activar.
    pub fn pack_usable(&self) -> bool {
        self.pack.state != PackStatus::NoPack
            && self.pack.state.is_playable()
            && self.rom.state.is_playable()
    }

    /// Lo que la sección del pack dice de él, con todas las palabras (#632).
    ///
    /// Es el motivo del validador, **salvo cuando la ROM es el problema**: ahí
    /// habla la ROM, porque «el pack se horneó para otra ROM» dice qué hacer
    /// —conseguir el que corresponde— y el grado general del pack, no.
    ///
    /// Acá vive lo que antes era el eje ROM de la ficha. No se pierde ninguno de
    /// los cuatro casos del validador: cambia de lugar la frase, no el
    /// diagnóstico. Y es una sola: dos renglones abajo de un semáforo es una
    /// pared de texto que a tres metros nadie lee.
    pub fn pack_reason(&self) -> &str {
        if self.pack.state == PackStatus::NoPack {
            return "";
        }
        // Una ROM que no coincide manda SIEMPRE, aunque el grado del pack sea
        // igual de malo: es el único de los dos que dice qué hacer —conseguir el
        // pack que corresponde—. El veredicto general, en ese mismo caso, dice
        // «1 error(es) impiden usar el pack», que es cierto y no lleva a ningún
        // lado. Es la misma precedencia que usa `motivo_corto_del_pack`, y tiene
        // que serlo: son la versión corta y la larga de la misma frase.
        if !self.rom.state.is_playable() {
            return &self.rom.reason;
        }
        // En el resto manda el peor, y entre iguales el pack: su motivo es el
        // del veredicto que la tarjeta también muestra, y las dos superficies
        // tienen que decir lo mismo del mismo pack.
        if self.pack.state == Self::worst_of(self.rom.state, self.pack.state) {
            return &self.pack.reason;
        }
        &self.rom.reason
    }

    /// Por qué el pack no se puede activar, **en una frase corta** (#631).
    ///
    /// Va al lado del conmutador de HD, que es donde el jugador se entera de que
    /// no puede activarlo: un control deshabilitado sin razón manda a probar de
    /// nuevo en vez de a arreglar lo que falta.
    ///
    /// Es una frase PROPIA y no el motivo del validador. Ése ya está abajo, en
    /// la fila del pack, y ahí tiene el ancho de la pantalla para decir «1
    /// error(es) impiden usar el pack en esta sesión: no es un .ay legible (ZIP
    /// inválido): invalid Zip archive: Could not find EOCD». Puesto al lado de
    /// un conmutador, ese mismo texto ocupa seis renglones de una columna
    /// angosta y **parte la fila de acciones en dos**. Al lado del control va
    /// qué pasa; el detalle, donde entra.
    ///
    /// Y distingue los dos casos, porque llevan a acciones distintas: un pack de
    /// otra ROM se arregla consiguiendo el que corresponde, y uno roto no se
    /// arregla. Cuando el pack se puede usar —o no hay— devuelve vacío: no hay
    /// nada que explicar.
    pub fn short_pack_reason(&self) -> &'static str {
        if self.pack.state == PackStatus::NoPack || self.pack_usable() {
            return "";
        }
        if !self.rom.state.is_playable() {
            return "El pack es de otra ROM";
        }
        "El pack no se puede usar"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// Los cuatro casos de la ROM son cuatro, no dos. «Este pack es de otro
    /// juego» y «este pack no dice de cuál es» son problemas opuestos: el
    /// primero se arregla consiguiendo el pack correcto y el segundo hay que
    /// probarlo. Colapsarlos en «sí/no» le quita al jugador la única pista.
    #[test]
    fn the_rom_axis_tells_the_four_cases_apart() {
        assert_eq!(rom_axis(&codes(&[]), false).state, PackStatus::NoPack);
        assert_eq!(rom_axis(&codes(&[]), true).state, PackStatus::Exact);
        assert_eq!(
            rom_axis(&codes(&["rom.undeclared"]), true).state,
            PackStatus::Warnings
        );
        assert_eq!(
            rom_axis(&codes(&["rom.unverified"]), true).state,
            PackStatus::Unverified
        );
        assert_eq!(
            rom_axis(&codes(&["rom.mismatch"]), true).state,
            PackStatus::Incompatible
        );
        // Y ninguno se queda sin motivo: un badge sin explicación obliga a la UI
        // a inventar el texto.
        for c in [
            vec![],
            codes(&["rom.undeclared"]),
            codes(&["rom.unverified"]),
            codes(&["rom.mismatch"]),
        ] {
            assert!(!rom_axis(&c, true).reason.is_empty());
        }
    }

    /// Lo PEOR manda, aunque venga de otro código en la misma lista: un pack que
    /// además de no declarar la ROM resulta ser de otro juego es incompatible, no
    /// «con advertencias».
    #[test]
    fn on_the_rom_the_worst_wins() {
        let e = rom_axis(&codes(&["rom.undeclared", "rom.mismatch"]), true);
        assert_eq!(e.state, PackStatus::Incompatible);
    }

    /// «No tengo core» y «todavía no lo comprobé» son distintos. Tratarlos igual
    /// haría que Play dijera «falta el core» durante el segundo que tarda el
    /// sondeo del arranque — y eso, en la pantalla que uno abre para entender,
    /// es una respuesta falsa.
    ///
    /// #642 agregó el tercero, que es el que faltaba: «lo comprobé y no sirve».
    #[test]
    fn the_core_axis_separates_no_from_not_yet() {
        assert_eq!(
            core_axis(&CoreAxis::Missing, "Mega Drive").state,
            PackStatus::Incompatible
        );
        assert_eq!(
            core_axis(&CoreAxis::Unchecked, "Mega Drive").state,
            PackStatus::Unverified
        );
        assert_eq!(
            core_axis(&CoreAxis::Usable, "Mega Drive").state,
            PackStatus::Exact
        );
        assert_eq!(
            core_axis(&CoreAxis::Unusable("es de 32 bits".into()), "Mega Drive").state,
            PackStatus::Incompatible
        );
        // Y el motivo nombra la consola: «no hay core» sin decir para qué no
        // ayuda a arreglarlo.
        assert!(
            core_axis(&CoreAxis::Missing, "Mega Drive")
                .reason
                .contains("Mega Drive")
        );
        // El del rechazo dice lo que dijo el sondeo, que es lo accionable.
        assert!(
            core_axis(&CoreAxis::Unusable("es de 32 bits".into()), "MD")
                .reason
                .contains("32 bits")
        );
    }

    /// **`bloquea()` y «Jugar» no pueden discrepar.** Son la misma pregunta en
    /// dos lugares —el `bool` que cruza a Slint y el grado del eje— y el día que
    /// se separen, la tarjeta va a estar apagada con el botón habilitado.
    #[test]
    fn what_blocks_in_cores_is_what_turns_off_play_on_the_card() {
        let all_items = [
            CoreAxis::Missing,
            CoreAxis::Unusable("x".into()),
            CoreAxis::Unchecked,
            CoreAxis::CheckFailed,
            CoreAxis::Undeclared,
            CoreAxis::Usable,
        ];
        for c in &all_items {
            let e = axes(&codes(&[]), false, PackStatus::NoPack, "", c, "MD");
            assert_eq!(e.is_playable(), !c.blocks_launch(), "{c:?}");
        }
    }

    /// #632 — LA RAZÓN DE LA ROM SE DICE EN LA SECCIÓN DEL PACK.
    ///
    /// Es el punto entero de la issue: el eje ROM nunca habló de la ROM del
    /// jugador, sino de para cuál se horneó el pack. Al lado del core se leía
    /// como «tu ROM está mal» y mandaba a arreglar lo que no está roto. Ahora la
    /// frase vive donde corresponde, y esto verifica que **no se perdió**:
    /// cambia de lugar el texto, no el diagnóstico.
    #[test]
    fn the_pack_motive_keeps_the_rom_reason() {
        // La ROM manda cuando es lo peor: dice qué hacer, y el grado general no.
        let e = axes(
            &codes(&["rom.mismatch"]),
            true,
            PackStatus::Incompatible,
            "1 error(es) impiden usar el pack",
            &CoreAxis::Usable,
            "MD",
        );
        assert!(
            e.pack_reason().contains("otra ROM"),
            "la razón de la ROM tiene que sobrevivir a la mudanza: {}",
            e.pack_reason()
        );

        // Entre iguales manda el pack: su motivo es el del veredicto que la
        // tarjeta también muestra, y las dos superficies no pueden discrepar.
        let e = axes(
            &codes(&["rom.undeclared"]),
            true,
            PackStatus::Warnings,
            "compatible con 1 advertencia",
            &CoreAxis::Usable,
            "MD",
        );
        assert_eq!(e.pack_reason(), "compatible con 1 advertencia");

        // Y cuando la ROM es lo único que tiene algo que decir, lo dice.
        let e = axes(
            &codes(&["rom.undeclared"]),
            true,
            PackStatus::Exact,
            "todo comprobado",
            &CoreAxis::Usable,
            "MD",
        );
        assert!(
            e.pack_reason().contains("no declara"),
            "una advertencia de la ROM sobre un pack exacto no puede callarse: {}",
            e.pack_reason()
        );

        // Sin pack no hay sección de la que hablar.
        let e = axes(
            &codes(&[]),
            false,
            PackStatus::NoPack,
            "",
            &CoreAxis::Usable,
            "MD",
        );
        assert_eq!(e.pack_reason(), "");
    }

    /// #631 — UN PACK QUE NO SIRVE NO PUEDE BLOQUEAR EL JUEGO.
    ///
    /// El defecto que cierra: `jugable()` salía de los tres ejes, así que un
    /// `.ay` de otro juego al lado de la ROM dejaba «Jugar» en gris sobre un
    /// juego que corre perfecto sin él. Y no había salida — apagar el conmutador
    /// de HD escribe el perfil de la próxima sesión y no recalcula el veredicto,
    /// y desde la grilla A abre la ficha—, así que el juego quedaba inalcanzable
    /// por culpa de un archivo que sobra.
    ///
    /// La incoherencia estaba a la vista: `SinPack` no contaba para el veredicto
    /// porque no tener pack no es un problema. Si no tenerlo no bloquea, tener
    /// uno malo tampoco: en los dos casos se juega el original.
    #[test]
    fn the_pack_does_not_decide_whether_you_play() {
        // El caso del defecto: pack de otro juego, core presente.
        let e = axes(
            &codes(&["rom.mismatch"]),
            true,
            PackStatus::Incompatible,
            "el pack se horneó para otra ROM",
            &CoreAxis::Usable,
            "MD",
        );
        assert!(
            e.is_playable(),
            "un pack incompatible NO puede bloquear el original"
        );
        assert!(!e.pack_usable(), "pero el pack tampoco se puede activar");
        assert_eq!(
            e.short_pack_reason(),
            "El pack es de otra ROM",
            "y el conmutador tiene que poder decir por qué, en una línea"
        );

        // Sin core no se juega: es el ÚNICO caso que apaga el botón.
        let e = axes(
            &codes(&[]),
            true,
            PackStatus::Exact,
            "todo bien",
            &CoreAxis::Missing,
            "MD",
        );
        assert!(!e.is_playable(), "sin core no hay nada que correr");

        // Y el core sin sondear no es un «no»: se puede intentar.
        let e = axes(
            &codes(&[]),
            false,
            PackStatus::NoPack,
            "",
            &CoreAxis::Unchecked,
            "MD",
        );
        assert!(
            e.is_playable(),
            "«todavía no lo comprobé» no puede leerse como «no anda»"
        );
    }

    /// #633 — CADA ESTADO DEL PACK OFRECE UNA ACCIÓN, Y SÓLO UNA.
    ///
    /// La tabla entera se prueba acá, sin abrir una ventana, que es la única
    /// forma de verificar cuatro estados por cuatro acciones sin volverse loco
    /// — el mismo criterio que `pad_router`.
    #[test]
    fn each_pack_state_offers_an_action() {
        use PackAction as A;
        use PackState as E;
        let cases = [
            //  hay    usable  activo   estado            acción
            // Sin pack, la acción es INSTALAR desde el disco (#678): el
            // «Conseguir» de AYTHER Play lleva al Hub; el de CE, al diálogo
            // que audita un .ay del jugador (ADR-005).
            (false, false, false, E::None, A::Install),
            (false, false, true, E::None, A::Install),
            (true, true, true, E::Active, A::Deactivate),
            (true, true, false, E::Disabled, A::Activate),
            (true, false, false, E::Unusable, A::NoOp),
            // El perfil dice que sí sobre un pack que no sirve: quedó de antes.
            // Manda el PACK — anunciar «Activo» sobre algo que Play va a dejar
            // afuera de la sesión es la ficha contradiciéndose sola.
            (true, false, true, E::Unusable, A::NoOp),
        ];
        for (has_gamepad, usable, active, state, action) in cases {
            let e = E::of(has_gamepad, usable, active);
            assert_eq!(e, state, "estado de ({has_gamepad},{usable},{active})");
            assert_eq!(e.action(), action, "acción de {state:?}");
        }
    }

    /// Los cuatro índices que cruzan a Slint son cuatro y distintos. Si dos
    /// colapsan, la ficha ofrece la acción de otro estado — y ésa es la clase de
    /// error que no falla, sólo hace lo que no era.
    #[test]
    fn the_indices_crossing_into_slint_do_not_collapse() {
        use PackState as E;
        let v: Vec<i32> = [E::None, E::Active, E::Disabled, E::Unusable]
            .iter()
            .map(|e| e.as_int())
            .collect();
        let mut u = v.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), v.len(), "dos estados comparten índice: {v:?}");
    }

    /// El pack se activa sólo si sirve, y «sin pack» no es «no sirve»: es que no
    /// hay nada que activar. Los dos dan `false`, pero por motivos distintos —y
    /// sólo uno de los dos tiene algo que explicar.
    #[test]
    fn the_pack_activates_only_if_it_works() {
        let no_pack_axes = axes(
            &codes(&[]),
            false,
            PackStatus::NoPack,
            "",
            &CoreAxis::Usable,
            "MD",
        );
        assert!(!no_pack_axes.pack_usable());
        assert_eq!(
            no_pack_axes.short_pack_reason(),
            "",
            "sin pack no hay nada que explicar: no falta nada"
        );

        let valid_pack = axes(
            &codes(&[]),
            true,
            PackStatus::Exact,
            "todo comprobado",
            &CoreAxis::Usable,
            "MD",
        );
        assert!(valid_pack.pack_usable());
        assert_eq!(
            valid_pack.short_pack_reason(),
            "",
            "un pack que anda no necesita disculparse"
        );

        // «No verificado» y «con advertencias» SÍ se pueden activar: son
        // «nadie lo comprobó» y «anda con reparos», no «no anda».
        for g in [PackStatus::Warnings, PackStatus::Unverified] {
            let e = axes(
                &codes(&[]),
                true,
                g,
                "algo que decir",
                &CoreAxis::Usable,
                "MD",
            );
            assert!(e.pack_usable(), "{g:?} tendría que poder activarse");
        }

        // Y la ROM que no coincide lo impide aunque el grado general no lo diga:
        // ese pack arranca y sustituye donde no va.
        let e = axes(
            &codes(&["rom.mismatch"]),
            true,
            PackStatus::Warnings,
            "con advertencias",
            &CoreAxis::Usable,
            "MD",
        );
        assert!(!e.pack_usable(), "un pack de otra ROM no se activa");
    }

    /// #632 — CADA SECCIÓN EXPLICA LO SUYO, Y NINGUNA HABLA POR LA OTRA.
    ///
    /// Antes había UNA frase para los tres ejes y elegía el problema más grave,
    /// con el core primero. Hacía falta porque tres frases abajo de tres badges
    /// eran una pared de texto; con dos secciones, cada una dice lo suyo al lado
    /// de su semáforo y la elección desaparece — que es mejor que acertarla.
    ///
    /// Lo que esto fija es que no se crucen: el estado del juego no puede
    /// terminar hablando del pack, ni el pack del core.
    #[test]
    fn each_section_explains_its_own() {
        // Falta el core Y el pack está roto: cada uno cuenta su historia.
        let e = axes(
            &codes(&["rom.mismatch"]),
            true,
            PackStatus::Incompatible,
            "el pack no sirve",
            &CoreAxis::Missing,
            "Mega Drive",
        );
        assert!(e.core.reason.contains("core"), "el estado habla del core");
        assert!(
            e.pack_reason().contains("otra ROM"),
            "y el pack, del pack: {}",
            e.pack_reason()
        );
        assert!(
            !e.core.reason.contains("pack"),
            "el estado del juego no puede explicar el pack: {}",
            e.core.reason
        );

        // Todo en orden: el pack sigue teniendo algo que decir.
        let e = axes(
            &codes(&[]),
            true,
            PackStatus::Exact,
            "todo comprobado",
            &CoreAxis::Usable,
            "Mega Drive",
        );
        assert_eq!(e.pack_reason(), "todo comprobado");
        assert!(e.core.reason.contains("Mega Drive"));
    }

    /// El eje del pack NO se recalcula: viene del veredicto general, el mismo que
    /// muestra la tarjeta. Si la ficha lo derivara aparte, las dos superficies
    /// podrían contradecirse sobre el mismo pack y el jugador tendría dos
    /// diagnósticos sin manera de elegir.
    #[test]
    fn the_pack_axis_is_the_overall_verdict() {
        let e = axes(
            &codes(&["rom.undeclared"]),
            true,
            PackStatus::Warnings,
            "compatible con 1 advertencia",
            &CoreAxis::Usable,
            "MD",
        );
        assert_eq!(e.pack.state, PackStatus::Warnings);
        assert_eq!(e.pack.reason, "compatible con 1 advertencia");
    }
}
