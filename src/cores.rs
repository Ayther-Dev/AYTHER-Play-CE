//! cores.rs — qué corre cada core, según el SONDEO y no según el archivo (#642).
//!
//! # El agujero que esto tapa
//!
//! Hasta #631 el eje del core era uno de tres y el veredicto salía del peor, así
//! que exagerar de a poco no se notaba. Desde #631 **el core decide solo si se
//! puede jugar**: la ficha dice «Compatible · Hay un core que corre Mega Drive»
//! y la tarjeta se apaga o no según eso. Ese eje pasó a valer lo que dice.
//!
//! Y lo que decía salía de `hay_core_para`, que comprobaba **que el archivo
//! existiera**. Nada más: ni que cargue, ni que sea de la arquitectura del
//! runtime, ni que corra esa consola. Un core de 32 bits, uno corrupto o uno de
//! otra consola daban «Compatible» y el juego no arrancaba — el error salía al
//! apretar A, que es el momento más caro para enterarse y el que #592 existía
//! para evitar.
//!
//! # Cómo se contesta ahora
//!
//! Con `core_probe::validar` (#589), que ya hacía exactamente esa comprobación
//! en dos pasos —el header PE sin cargar nada, y `ayther_runtime --probe-core`
//! fuera de proceso— y cuyo resultado ya se usaba para graduar los packs. Lo
//! único que faltaba era que la ficha lo mirara.
//!
//! **El sondeo es del CORE, no del juego.** Se hace una vez por ruta de core y
//! se guarda acá; catorce juegos de Mega Drive comparten la respuesta. Lanzar un
//! proceso por tarjeta es justamente lo que #595 evitó.
//!
//! # Y por qué son seis estados y no un `bool`
//!
//! Porque «no tengo core» y «todavía no lo comprobé» no son lo mismo, y
//! colapsarlos haría que Play dijera «Falta el core» de toda la biblioteca
//! durante el segundo que tarda el sondeo del arranque. Tampoco son lo mismo
//! «este core no sirve» y «no se pudo comprobar»: el primero es un no, el
//! segundo es un problema de la instalación de Ayther, y llevan a acciones
//! distintas. Sólo los dos primeros impiden jugar (`bloquea`).

use crate::config::{self, Config};
use crate::core_probe::Verdict;
use crate::tr;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// El estado del eje
// ---------------------------------------------------------------------------

/// En qué está el core de UNA plataforma.
///
/// El número que lleva cada uno cruza a Slint y **no es decorativo**: la UI
/// pregunta `core <= 1` para saber si tiene que apagar la tarjeta y deshabilitar
/// «Jugar». El oráculo `la_ui_usa_los_mismos_numeros_que_este_enum` lo ata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreAxis {
    /// No hay ningún core configurado que corra esta consola, o la ruta que hay
    /// apunta a un archivo que no está. **Bloquea.**
    Missing,
    /// Hay un archivo y el sondeo lo rechazó: otra arquitectura, no es un core,
    /// otra API de libretro, o es un core de otra consola. **Bloquea.**
    ///
    /// Lleva el remedio —qué hacer— y no el fallo. «No se pudo cargar el core»
    /// no es un mensaje: es el registro de que nadie pensó en el caso.
    Unusable(String),
    /// El sondeo todavía no terminó. Un «todavía no», no un «no».
    Unchecked,
    /// Se intentó sondear y no se pudo: falta el runtime, o murió sin decir
    /// nada. **No bloquea**: que Ayther no pueda comprobar algo no es una razón
    /// para no dejar jugar.
    ///
    /// Sin el detalle del fallo a propósito. El detalle nombra el ejecutable que
    /// no se pudo correr, y una ruta de Windows lleva adentro el nombre de
    /// usuario: no va a una pantalla que se mira a tres metros. Va al log, que
    /// redacta (#621), y de ahí al paquete de diagnóstico (#622).
    CheckFailed,
    /// Carga, es un core de libretro, pero no declara ninguna extensión que
    /// conozcamos, así que no sabemos qué consolas corre. Se acepta igual —es lo
    /// que ya hace el onboarding (#589)— pero no se afirma que sirva.
    Undeclared,
    /// El sondeo dice que este core corre esta consola.
    Usable,
}

impl CoreAxis {
    /// El entero que cruza a Slint. `0` y `1` son los que bloquean, y ese orden
    /// es lo que le permite a la UI preguntar `core <= 1` en vez de enumerar.
    pub fn as_int(&self) -> i32 {
        match self {
            CoreAxis::Missing => 0,
            CoreAxis::Unusable(_) => 1,
            CoreAxis::Unchecked => 2,
            CoreAxis::CheckFailed => 3,
            CoreAxis::Undeclared => 4,
            CoreAxis::Usable => 5,
        }
    }

    /// ¿Este estado impide jugar?
    ///
    /// Sale del número y no de un `matches!` aparte a propósito: si mañana
    /// alguien agrega un estado en el medio, esto y la UI se equivocan **juntos**
    /// y el oráculo lo agarra. Dos listas separadas se desincronizan en silencio.
    ///
    /// La UI hace la misma pregunta con `Semaforo.core-bloquea` para apagar la
    /// tarjeta; ésta es la que además **impide el lanzamiento**, porque el botón
    /// en gris no es la única puerta: A del mando entra por otro lado.
    pub fn blocks_launch(&self) -> bool {
        self.as_int() <= 1
    }
}

// ---------------------------------------------------------------------------
// Lo que ya se sondeó
// ---------------------------------------------------------------------------

/// Los sondeos hechos en esta corrida, por RUTA de core.
///
/// Por ruta y no por plataforma porque el sondeo es del archivo: un mismo core
/// puede estar anotado bajo tres consolas, y sondearlo tres veces serían tres
/// procesos para la misma respuesta.
#[derive(Debug, Clone, Default)]
pub struct Probes {
    by_core: BTreeMap<PathBuf, Verdict>,
}

impl Probes {
    pub const fn new() -> Self {
        Probes {
            by_core: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, core: PathBuf, v: Verdict) {
        self.by_core.insert(core, v);
    }

    pub fn get(&self, core: &Path) -> Option<&Verdict> {
        self.by_core.get(core)
    }
}

/// Los sondeos de esta corrida.
///
/// Global porque el dato es de la MÁQUINA, no de una pantalla: la grilla, la
/// ficha y el menú tienen que contestar lo mismo. Pasarlo por parámetro por los
/// puntos donde se re-proyecta la biblioteca sería una copia por punto y una
/// oportunidad de que una quede vieja.
///
/// La decisión no vive acá: vive en `eje`, que es pura y recibe los sondeos. El
/// estado global es sólo dónde se guardan.
static PROBES: Mutex<Probes> = Mutex::new(Probes::new());

/// Anota lo que el sondeo dijo de un core.
pub fn record(core: PathBuf, v: Verdict) {
    if let Ok(mut s) = PROBES.lock() {
        s.record(core, v);
    }
}

/// El eje del core para esta plataforma, con lo sondeado hasta ahora.
///
/// Un lock envenenado se lee como «no se sondeó nada»: `SinComprobar` no
/// bloquea, así que el peor caso es un semáforo prudente y no una biblioteca
/// entera apagada por un panic en otro hilo.
pub fn state(cfg: &Config, platform: &str) -> CoreAxis {
    match PROBES.lock() {
        Ok(s) => axis(cfg, &s, platform),
        Err(_) => CoreAxis::Unchecked,
    }
}

/// Presentation uses the last probe. Launch preparation checks disk again.
pub fn cached_state(cfg: &Config, platform: &str) -> CoreAxis {
    let Some(path) = config::core_for(cfg, platform) else {
        return CoreAxis::Missing;
    };
    match PROBES.lock() {
        Ok(probes) => verdict_axis(probes.get(&path), platform),
        Err(_) => CoreAxis::Unchecked,
    }
}

// ---------------------------------------------------------------------------
// La decisión
// ---------------------------------------------------------------------------

/// El eje del core, dados una configuración y unos sondeos. **Pura**: es la
/// función que los oráculos pueden correr sin ventana, sin runtime y sin cores.
///
/// El orden de las preguntas es el orden en que se abaratan las respuestas:
/// primero si hay ruta, después si el archivo está, y recién ahí lo que dijo el
/// proceso. Al revés habría que sondear para poder decir «no hay core».
pub fn axis(cfg: &Config, probes: &Probes, platform: &str) -> CoreAxis {
    let Some(path) = config::core_for(cfg, platform) else {
        return CoreAxis::Missing;
    };
    // Una ruta que quedó apuntando a un core borrado, o a un disco
    // desconectado, no es un core. Se contesta sin mirar el sondeo porque el
    // sondeo de ayer no dice nada del archivo de hoy.
    if !path.exists() {
        return CoreAxis::Missing;
    }
    verdict_axis(probes.get(&path), platform)
}

fn verdict_axis(verdict: Option<&Verdict>, platform: &str) -> CoreAxis {
    let Some(v) = verdict else {
        return CoreAxis::Unchecked;
    };
    match v {
        Verdict::Missing(_) => CoreAxis::Missing,
        Verdict::ProbeFailed(_) => CoreAxis::CheckFailed,
        Verdict::Valid(i) => {
            if i.supports(platform) {
                CoreAxis::Usable
            } else if i.platforms().is_empty() {
                // No declara extensiones conocidas. No es un no: es que el core
                // no dice de qué es. Bloquear acá dejaría sin jugar a quien usa
                // un core que el onboarding aceptó.
                CoreAxis::Undeclared
            } else {
                let is_usable: Vec<String> = i.platforms().into_iter().collect();
                CoreAxis::Unusable(tr!(
                    "El core configurado corre {}, no {}. Elegí uno en Ajustes.",
                    is_usable.join(", "),
                    platform
                ))
            }
        }
        // Los tres que el sondeo rechaza. El texto es el REMEDIO de
        // `core_probe`, que ya está escrito para que el jugador pueda actuar:
        // «Descargá la versión x64 del core: la que elegiste es x86 (32 bits)».
        other => CoreAxis::Unusable(other.remedy()),
    }
}

/// Las rutas que hay que sondear al arrancar: una por core configurado.
///
/// Deduplicadas —un core anotado bajo tres consolas es un proceso, no tres— y
/// sin las que no existen, que ya se contestan sin sondear.
///
/// El `default_core` entra siempre: con el mapa vacío es el único que hay, y con
/// el mapa lleno sigue siendo el que grada los packs (#595).
pub fn to_probe(cfg: &Config) -> Vec<PathBuf> {
    let mut paths: BTreeSet<PathBuf> = cfg.cores.values().cloned().collect();
    paths.insert(cfg.default_core.clone());
    paths.into_iter().filter(|r| r.exists()).collect()
}

#[cfg(test)]
mod tests;
