//! gamepad.rs — el mando mueve la interfaz (#311).
//!
//! Slint navega con teclado; nadie lee el mando. Este módulo pone el puente:
//! un hilo lee gilrs y emite acciones ABSTRACTAS —arriba, abajo, izquierda,
//! derecha, aceptar, volver— que el caller conecta a lo que ya hace el teclado.
//!
//! Acciones y no botones a propósito: la pantalla no tiene por qué saber que en
//! un mando de Xbox el botón sur se llama South y en uno de Nintendo está del
//! otro lado. El mapeo vive acá, en un solo lugar.
//!
//! **El stick también navega, con zona muerta y sin repetición automática.** Un
//! stick inclinado emite eventos continuos mientras esté inclinado; sin el
//! latch, empujarlo una vez recorrería la lista entera. Se emite UNA acción al
//! cruzar el umbral y no se vuelve a emitir hasta que el stick vuelve al
//! centro, que es lo que la mano espera de un menú.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Lo que la interfaz entiende. Sin nombres de botones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadAction {
    Up,
    Down,
    Left,
    Right,
    /// El botón de acción (A / sur).
    Accept,
    /// Volver (B / este).
    Back,
    /// Start. NO es lo mismo que Accept y por eso está aparte: en un menú de
    /// opciones, A cambia la fila donde está el cursor y Start arranca la
    /// partida. Un botón que a veces cambia una opción y a veces entra al juego
    /// es el que hace que alguien empiece sin querer.
    Start,
    /// El botón de arriba (Y / norte): la acción secundaria de la pantalla.
    /// Hoy atiende el aviso de actualización, que antes sólo se podía tocar
    /// con el mouse (#586).
    Menu,
    /// El botón de la izquierda (X / oeste). #627 — lo pide el onboarding
    /// nuevo, donde el pie del diseño dice «X Agregar»: con A ocupado en
    /// continuar, la acción del paso necesitaba un botón propio.
    SecondaryAction,
    /// Gatillos superiores (LB / RB): saltar de sección sin pasar por el
    /// cursor. En una TV, alcanzar una pestaña con el stick es el gesto que
    /// más cansa (spec §9).
    PrevTab,
    NextTab,
}

/// Cuánto hay que inclinar el stick para que cuente. 0,5 y no 0,2: un stick
/// gastado descansa lejos del centro, y un umbral bajo haría que el menú se
/// moviera solo mientras nadie lo toca.
const AXIS_THRESHOLD: f32 = 0.5;

/// El nombre SDL de un botón crudo, para la captura de Ajustes › Controles
/// del juego. Es el vocabulario del contrato `--input-map` (el runtime lo
/// resuelve con SDL_GetGamepadButtonFromString), así que la tabla vive acá,
/// donde ya vive el resto del conocimiento del mando.
///
/// `None` para lo que no se asigna: las direcciones (cruceta y stick son
/// dirección, no elección) y lo que gilrs no nombra.
pub fn sdl_button(b: gilrs::Button) -> Option<&'static str> {
    use gilrs::Button as B;
    Some(match b {
        B::South => "south",
        B::East => "east",
        B::West => "west",
        B::North => "north",
        // En gilrs `LeftTrigger` es el BUMPER (LB); el gatillo analógico es
        // `LeftTrigger2` — la misma trampa que ya documenta el ruteo.
        B::LeftTrigger => "leftshoulder",
        B::RightTrigger => "rightshoulder",
        B::Start => "start",
        B::Select => "back",
        _ => return None,
    })
}

/// La tecla que el pie DIBUJA, convertida a la acción que el router entiende.
///
/// Es lo que vuelve al pie una BOTONERA: un click del mouse en «B Cerrar»
/// entra por `enrutar` igual que el botón B del mando, así que mouse, teclado
/// y mando no pueden divergir — divergen sólo si alguien escribe un camino
/// aparte, y esta función existe para que no haga falta.
///
/// `None` para lo que no es un botón: los hints de dirección llevan un ícono y
/// `tecla` vacía, y un click ahí no puede inventar una acción.
pub fn hint_action(key: &str) -> Option<PadAction> {
    Some(match key {
        "A" => PadAction::Accept,
        "B" => PadAction::Back,
        "X" => PadAction::SecondaryAction,
        "Y" => PadAction::Menu,
        "Start" => PadAction::Start,
        "LB" => PadAction::PrevTab,
        "RB" => PadAction::NextTab,
        _ => return None,
    })
}

/// Arranca el hilo que lee el mando. `on_action` corre EN ESE HILO — el caller
/// se encarga de saltar al hilo de la UI (`slint::invoke_from_event_loop`).
///
/// Devuelve un `AtomicBool` para pararlo. Sin gamepad conectado el hilo arranca
/// igual y no hace nada: enchufar el mando con Play abierto tiene que funcionar,
/// y para eso hay que estar escuchando desde antes.
/// `mando` recibe el nombre del mando conectado, o queda vacio si no hay
/// ninguno. Lo escribe el mismo hilo que ya escucha los eventos: preguntarle a
/// gilrs desde otro lado obligaria a una segunda instancia, y hay plataformas
/// donde la segunda no ve nada. El paso 2 del onboarding (#588) lo muestra.
///
/// `on_conexion` avisa si HAY mando — al arrancar y en cada enchufe o
/// desenchufe. Es lo que decide qué dibuja el pie: las píldoras del mando, o
/// las teclas del teclado cuando no hay con qué apretarlas. Corre en este
/// hilo, como `on_action`: el caller salta al hilo de la UI.
///
/// `captura` es el modo del diálogo de controles: mientras está puesto, el
/// PRÓXIMO botón con nombre no navega — viaja crudo por `on_boton` (su nombre
/// SDL) y apaga el modo. Las direcciones siguen navegando: mover el foco del
/// diálogo mientras se espera un botón es lo que la mano intenta primero.
pub fn start<F, B, C>(
    on_action: F,
    capture: Arc<AtomicBool>,
    on_button: B,
    on_connection: C,
    gamepad_name: Arc<Mutex<String>>,
) -> Arc<AtomicBool>
where
    F: Fn(PadAction) + Send + 'static,
    B: Fn(&'static str) + Send + 'static,
    C: Fn(bool) + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);

    std::thread::spawn(move || {
        let mut gil = match gilrs::Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                // Sin mando la aplicación sigue andando con teclado: esto no es
                // un error fatal, es una capacidad que no está.
                eprintln!("[gamepad] no disponible: {e}");
                return;
            }
        };
        // Latch por EJE, no por evento: hay que saber si el stick ya estaba
        // inclinado para no volver a emitir.
        let mut lx = 0i8; // -1 izquierda · 0 centro · 1 derecha
        let mut ly = 0i8;

        // Lo que ya estaba enchufado cuando Play arrancó: `Connected` sólo
        // llega para los que se enchufan DESPUÉS, así que sin esto el paso de
        // mandos diría «ninguno detectado» con el mando en la mano.
        let record_connection = |gil: &gilrs::Gilrs, m: &Arc<Mutex<String>>| -> bool {
            let name = gil
                .gamepads()
                .find(|(_, g)| g.is_connected())
                .map(|(_, g)| g.name().to_string())
                .unwrap_or_default();
            let has_gamepad = !name.is_empty();
            if let Ok(mut s) = m.lock() {
                *s = name;
            }
            has_gamepad
        };
        on_connection(record_connection(&gil, &gamepad_name));

        while !stop2.load(Ordering::Relaxed) {
            let mut refresh_connection = false;
            while let Some(ev) = gil.next_event() {
                use gilrs::{Axis, Button, EventType};
                match ev.event {
                    EventType::ButtonPressed(b, _) => {
                        // La captura come el botón ANTES del ruteo: es un dato
                        // («este botón quiero»), no un gesto. Sólo los que se
                        // pueden asignar; una dirección sigue navegando.
                        if capture.load(Ordering::Relaxed)
                            && let Some(name) = sdl_button(b)
                        {
                            capture.store(false, Ordering::Relaxed);
                            on_button(name);
                            continue;
                        }
                        let a = match b {
                            Button::DPadUp => Some(PadAction::Up),
                            Button::DPadDown => Some(PadAction::Down),
                            Button::DPadLeft => Some(PadAction::Left),
                            Button::DPadRight => Some(PadAction::Right),
                            Button::South => Some(PadAction::Accept),
                            Button::East => Some(PadAction::Back),
                            Button::Start => Some(PadAction::Start),
                            Button::North => Some(PadAction::Menu),
                            Button::West => Some(PadAction::SecondaryAction),
                            // `LeftTrigger` es el BUMPER (LB) en gilrs; el
                            // gatillo analógico es `LeftTrigger2`. Confundirlos
                            // deja los hombros mudos y nadie sabe por qué.
                            Button::LeftTrigger => Some(PadAction::PrevTab),
                            Button::RightTrigger => Some(PadAction::NextTab),
                            _ => None,
                        };
                        if let Some(a) = a {
                            on_action(a);
                        }
                    }
                    EventType::AxisChanged(axis, v, _) => {
                        let dir = if v > AXIS_THRESHOLD {
                            1i8
                        } else if v < -AXIS_THRESHOLD {
                            -1i8
                        } else {
                            0i8
                        };
                        match axis {
                            Axis::LeftStickX
                                // Sólo al CRUZAR el umbral: mientras siga
                                // inclinado no se repite.
                                if dir != lx => {
                                    lx = dir;
                                    if dir == 1 {
                                        on_action(PadAction::Right);
                                    }
                                    if dir == -1 {
                                        on_action(PadAction::Left);
                                    }
                                }
                            Axis::LeftStickY
                                if dir != ly => {
                                    ly = dir;
                                    // Y positivo es ARRIBA en gilrs.
                                    if dir == 1 {
                                        on_action(PadAction::Up);
                                    }
                                    if dir == -1 {
                                        on_action(PadAction::Down);
                                    }
                                }
                            _ => {}
                        }
                    }
                    // Enchufar o desenchufar el mando con Play abierto NO
                    // reinicia nada: gilrs mantiene el registro y los eventos
                    // del mando nuevo entran por el mismo camino (spec §14:
                    // «desconectar y reconectar no obliga a reiniciar»).
                    EventType::Connected => {
                        eprintln!("[gamepad] mando conectado");
                        // Se anota DESPUES de drenar los eventos: acá `gil`
                        // está prestado por el iterador.
                        refresh_connection = true;
                    }
                    EventType::Disconnected => {
                        eprintln!("[gamepad] mando desconectado");
                        // Se RE-ANOTA en vez de limpiar a secas: con dos mandos
                        // enchufados, desenchufar uno no puede dejar el pie en
                        // modo teclado mientras el otro sigue en la mano.
                        refresh_connection = true;
                    }
                    _ => {}
                }
            }
            if refresh_connection {
                on_connection(record_connection(&gil, &gamepad_name));
            }
            // 8 ms ≈ 120 Hz: más rápido no se percibe en un menú y quema
            // batería en un handheld; más lento se siente pegajoso.
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    });

    stop
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La tabla de captura habla el vocabulario del CONTRATO: los nombres que
    /// `--input-map` espera, con la trampa del bumper incluida — en gilrs
    /// `LeftTrigger` es LB, no el gatillo.
    #[test]
    fn capture_yields_the_contract_names() {
        use gilrs::Button as B;
        assert_eq!(sdl_button(B::South), Some("south"));
        assert_eq!(sdl_button(B::East), Some("east"));
        assert_eq!(sdl_button(B::LeftTrigger), Some("leftshoulder"));
        assert_eq!(sdl_button(B::Select), Some("back"));
        assert_eq!(sdl_button(B::DPadUp), None, "una dirección no se asigna");
        assert_eq!(
            sdl_button(B::LeftTrigger2),
            None,
            "el gatillo analógico tampoco"
        );
    }

    /// CADA TECLA QUE EL PIE DIBUJA HACE ALGO al clickearla, y hace LO MISMO
    /// que el botón del mando que dibuja. El pie de #627 es clickeable desde
    /// siempre; lo que esto fija es que el click no tenga un camino propio.
    #[test]
    fn each_footer_key_maps_to_its_button() {
        assert_eq!(hint_action("A"), Some(PadAction::Accept));
        assert_eq!(hint_action("B"), Some(PadAction::Back));
        assert_eq!(hint_action("X"), Some(PadAction::SecondaryAction));
        assert_eq!(hint_action("Y"), Some(PadAction::Menu));
        assert_eq!(hint_action("Start"), Some(PadAction::Start));
        assert_eq!(hint_action("LB"), Some(PadAction::PrevTab));
        assert_eq!(hint_action("RB"), Some(PadAction::NextTab));
        // Un hint de dirección lleva ícono y tecla vacía: el click ahí no
        // puede inventar una acción.
        assert_eq!(hint_action(""), None);
        assert_eq!(
            hint_action("Enter"),
            None,
            "el pie manda la tecla del PAD aunque dibuje la del teclado"
        );
    }

    /// El latch del stick es lo que impide que empujarlo una vez recorra la
    /// lista entera. Se prueba la REGLA, sin gilrs: dado un valor de eje y el
    /// estado anterior, ¿se emite o no?
    fn axis_step(prev: i8, v: f32) -> (i8, bool) {
        let dir = if v > AXIS_THRESHOLD {
            1
        } else if v < -AXIS_THRESHOLD {
            -1
        } else {
            0
        };
        // Se emite al CAMBIAR y sólo hacia un lado: volver al centro actualiza
        // el latch pero no es una acción. Sin la segunda condición, soltar el
        // stick movería el menú una vez más — que es el defecto que más se
        // siente y el más fácil de escribir sin querer.
        (dir, dir != prev && dir != 0)
    }

    #[test]
    fn the_stick_does_not_repeat_while_still_tilted() {
        let (d1, emit1) = axis_step(0, 0.9);
        assert!(emit1 && d1 == 1, "al cruzar el umbral se emite");
        let (d2, emit2) = axis_step(d1, 0.95);
        assert!(!emit2 && d2 == 1, "seguir inclinado NO vuelve a emitir");
        let (d3, emit3) = axis_step(d2, 0.0);
        assert!(!emit3 && d3 == 0, "volver al centro no emite nada");
        let (_, emit4) = axis_step(d3, 0.9);
        assert!(emit4, "y volver a inclinarlo sí");
    }

    #[test]
    fn the_dead_zone_ignores_rest() {
        // Un stick gastado descansa lejos del centro: con un umbral bajo el
        // menú se movería solo mientras nadie lo toca.
        let (d, emit) = axis_step(0, 0.3);
        assert!(!emit && d == 0);
        let (d2, emit2) = axis_step(0, -0.49);
        assert!(!emit2 && d2 == 0);
    }

    #[test]
    fn crossing_from_one_side_to_the_other_emits_once() {
        let (d1, _) = axis_step(0, 0.9);
        let (d2, emit) = axis_step(d1, -0.9);
        assert!(emit && d2 == -1, "de derecha a izquierda: una sola acción");
    }
}
