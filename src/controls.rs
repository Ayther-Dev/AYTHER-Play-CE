//! controls.rs — los controles del JUEGO, elegidos por el jugador.
//!
//! Ajustes › «Controles del juego»: qué tecla del teclado y qué botón del
//! mando hace cada acción del RetroPad DENTRO de la partida. No confundir con
//! la gramática del chasis (`pad_router`): esto no mueve menús, mueve a Sonic.
//!
//! # El contrato
//!
//! El runtime lee `--input-map <controles.toml>`
//! (`<AYTHER_RUNTIME_SOURCE>/src/input_map.cpp`)
//! con dos secciones que hablan en NOMBRES de SDL: `[keyboard] a = "K"` es un
//! scancode y `[gamepad] a = "north"` un botón. Las claves (`up`…`select`) son
//! las de esa tabla, y un oráculo de acá las compara contra el archivo del
//! runtime leyéndolo: dos copias del contrato que no se comparan son dos
//! contratos.
//!
//! # Sin huecos: el conflicto INTERCAMBIA
//!
//! Asignarle a Saltar la tecla que ya usa Disparar no puede dejar a Disparar
//! sin tecla —una acción muda se descubre en medio de la partida— ni duplicar
//! la tecla —dos acciones a la vez es un bug de otro juego—. Se INTERCAMBIAN:
//! Disparar se queda con la que Saltar tenía. El mapa siempre está completo,
//! que es además lo único que el TOML generado sabe decir.

use std::collections::BTreeMap;
use std::path::Path;

/// Una acción del juego: la clave del contrato, el id de RetroPad, y sus
/// valores de fábrica (los mismos de `input_map.cpp` — hay un oráculo).
/// `boton_default` es `None` en las direcciones: la cruceta no se remapea.
pub struct Action {
    pub id: &'static str,
    pub default_key: &'static str,
    pub default_button: Option<&'static str>,
}

/// Las 12, en el orden en que se muestran: primero moverse, después los
/// botones con nombre, al final Start y Select.
pub const ACTIONS: [Action; 12] = [
    Action {
        id: "up",
        default_key: "Up",
        default_button: None,
    },
    Action {
        id: "down",
        default_key: "Down",
        default_button: None,
    },
    Action {
        id: "left",
        default_key: "Left",
        default_button: None,
    },
    Action {
        id: "right",
        default_key: "Right",
        default_button: None,
    },
    Action {
        id: "b",
        default_key: "Z",
        default_button: Some("south"),
    },
    Action {
        id: "a",
        default_key: "X",
        default_button: Some("east"),
    },
    Action {
        id: "y",
        default_key: "A",
        default_button: Some("west"),
    },
    Action {
        id: "x",
        default_key: "S",
        default_button: Some("north"),
    },
    Action {
        id: "l",
        default_key: "Q",
        default_button: Some("leftshoulder"),
    },
    Action {
        id: "r",
        default_key: "W",
        default_button: Some("rightshoulder"),
    },
    Action {
        id: "start",
        default_key: "Return",
        default_button: Some("start"),
    },
    Action {
        id: "select",
        default_key: "Right Shift",
        default_button: Some("back"),
    },
];

/// Lo que la fila muestra como nombre de la acción. En el vocabulario del
/// RetroPad, que es el que el juego entiende; el nombre «de la consola» varía
/// por plataforma y mentiría en la mitad de ellas.
pub fn action_label(id: &str) -> String {
    match id {
        "up" => tr!("Arriba"),
        "down" => tr!("Abajo"),
        "left" => tr!("Izquierda"),
        "right" => tr!("Derecha"),
        "b" => "B".into(),
        "a" => "A".into(),
        "y" => "Y".into(),
        "x" => "X".into(),
        "l" => "L".into(),
        "r" => "R".into(),
        "start" => "Start".into(),
        "select" => "Select".into(),
        _ => id.to_string(),
    }
}

/// El nombre de una tecla SDL, como lo lee una persona. Los nombres de SDL ya
/// son legibles («K», «Space», «Right Shift»); acá sólo se traducen los que el
/// resto del chasis ya nombra en castellano.
pub fn key_label(sdl_name: &str) -> String {
    match sdl_name {
        "Space" => tr!("Espacio"),
        "Return" => tr!("Enter"),
        "Backspace" => tr!("Retroceso"),
        "Up" => "↑".into(),
        "Down" => "↓".into(),
        "Left" => "←".into(),
        "Right" => "→".into(),
        "Right Shift" => tr!("Shift der."),
        "Left Shift" => tr!("Shift izq."),
        other => other.to_string(),
    }
}

/// El nombre de un botón SDL, dicho como se lee en el mando (disposición
/// Xbox, que es la de SDL).
pub fn button_label(sdl_name: &str) -> String {
    match sdl_name {
        "south" => "A".into(),
        "east" => "B".into(),
        "west" => "X".into(),
        "north" => "Y".into(),
        "leftshoulder" => "LB".into(),
        "rightshoulder" => "RB".into(),
        "start" => "Start".into(),
        "back" => "Back".into(),
        other => other.to_string(),
    }
}

/// La tecla que Slint entrega, convertida al nombre de scancode que SDL
/// entiende. `None` = no asignable (se sigue esperando): teclas muertas, F-keys
/// que el chasis usa, y Escape, que es cómo se CANCELA la captura.
///
/// La UI ya tradujo sus constantes especiales a nombres (`"Up"`, `"Return"`…);
/// lo que llega como carácter suelto se normaliza acá.
pub fn capture_key_name(text: &str) -> Option<String> {
    // Los nombres que la UI manda por las teclas especiales, tal cual son
    // nombres de SDL.
    const NAMES: [&str; 12] = [
        "Up",
        "Down",
        "Left",
        "Right",
        "Return",
        "Tab",
        "Space",
        "Backspace",
        "Right Shift",
        "Left Shift",
        "Left Ctrl",
        "Left Alt",
    ];
    if NAMES.contains(&text) {
        return Some(text.to_string());
    }
    let mut ch = text.chars();
    let c = ch.next()?;
    if ch.next().is_some() {
        return None; // más de un carácter y no es un nombre conocido
    }
    if c == ' ' {
        return Some("Space".into());
    }
    if c.is_ascii_alphanumeric() {
        // SDL nombra las letras en mayúscula («K») y los dígitos tal cual.
        return Some(c.to_ascii_uppercase().to_string());
    }
    // Signos que SDL nombra por su carácter (`,` `.` `-` `=` `;` …).
    if c.is_ascii_punctuation() {
        return Some(c.to_string());
    }
    None
}

/// Lo elegido, encima de los defaults. Los mapas guardan SÓLO lo distinto de
/// fábrica —es lo que hace que «Restaurar» sea borrar, y que el TOML de un
/// jugador que nunca tocó nada no exista.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Controls {
    pub keys: BTreeMap<String, String>,
    pub buttons: BTreeMap<String, String>,
}

impl Controls {
    pub fn of(keys: &BTreeMap<String, String>, buttons: &BTreeMap<String, String>) -> Self {
        Controls {
            keys: keys.clone(),
            buttons: buttons.clone(),
        }
    }

    fn action(id: &str) -> Option<&'static Action> {
        ACTIONS.iter().find(|a| a.id == id)
    }

    /// La tecla vigente de una acción: la elegida, o la de fábrica.
    pub fn key_of(&self, id: &str) -> String {
        self.keys.get(id).cloned().unwrap_or_else(|| {
            Self::action(id)
                .map(|a| a.default_key.to_string())
                .unwrap_or_default()
        })
    }

    /// El botón vigente, o `None` si la acción es una dirección (no se remapea).
    pub fn button_of(&self, id: &str) -> Option<String> {
        let a = Self::action(id)?;
        let d = a.default_button?;
        Some(
            self.buttons
                .get(id)
                .cloned()
                .unwrap_or_else(|| d.to_string()),
        )
    }

    /// Asigna `tecla` a `clave`. Si otra acción ya la tenía, INTERCAMBIAN: el
    /// mapa queda completo siempre — ni acciones mudas ni teclas repetidas.
    pub fn assign_key(&mut self, id: &str, key: &str) {
        if Self::action(id).is_none() {
            return;
        }
        let previous = self.key_of(id);
        if previous == key {
            return;
        }
        if let Some(conflicting_action) = ACTIONS
            .iter()
            .find(|a| a.id != id && self.key_of(a.id) == key)
        {
            self.set_key(conflicting_action.id, &previous);
        }
        self.set_key(id, key);
    }

    /// Ídem para el botón del mando. Las direcciones no participan: ni reciben
    /// botón ni se lo pueden robar.
    pub fn assign_button(&mut self, id: &str, button: &str) {
        let Some(previous) = self.button_of(id) else {
            return;
        };
        if previous == button {
            return;
        }
        if let Some(conflicting_action) = ACTIONS
            .iter()
            .filter(|a| a.id != id)
            .find(|a| self.button_of(a.id).as_deref() == Some(button))
        {
            self.set_button(conflicting_action.id, &previous);
        }
        self.set_button(id, button);
    }

    fn set_key(&mut self, id: &str, key: &str) {
        let default = Self::action(id).map(|a| a.default_key).unwrap_or("");
        if key == default {
            self.keys.remove(id);
        } else {
            self.keys.insert(id.into(), key.into());
        }
    }

    fn set_button(&mut self, id: &str, button: &str) {
        let default = Self::action(id)
            .and_then(|a| a.default_button)
            .unwrap_or("");
        if button == default {
            self.buttons.remove(id);
        } else {
            self.buttons.insert(id.into(), button.into());
        }
    }

    /// ¿Hay algo distinto de fábrica? Decide si el TOML se escribe y la flag
    /// viaja: sin cambios, el runtime ya sabe qué hacer solo.
    pub fn has_changes(&self) -> bool {
        !self.keys.is_empty() || !self.buttons.is_empty()
    }

    /// Vuelve todo a fábrica.
    pub fn restore(&mut self) {
        self.keys.clear();
        self.buttons.clear();
    }

    /// El `controles.toml` del contrato, COMPLETO: las 12 teclas y los 8
    /// botones vigentes. Podría llevar sólo los cambios —el runtime rellena—
    /// pero completo es verificable a ojo, y el archivo es la única
    /// documentación de quien lo edita a mano.
    pub fn toml(&self) -> String {
        let mut s = String::from(
            "# GENERADO por AYTHER Play (Ajustes > Controles del juego).\n\
             # Nombres de SDL: scancodes en [keyboard], botones en [gamepad].\n\n\
             [keyboard]\n",
        );
        for a in &ACTIONS {
            s.push_str(&format!("{} = \"{}\"\n", a.id, self.key_of(a.id)));
        }
        s.push_str("\n[gamepad]\n");
        for a in &ACTIONS {
            if let Some(b) = self.button_of(a.id) {
                s.push_str(&format!("{} = \"{}\"\n", a.id, b));
            }
        }
        s
    }

    /// Escribe el TOML donde el lanzamiento lo va a mandar (`--input-map`).
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(path, self.toml())
    }
}

// ---------------------------------------------------------------------------
// El diálogo: qué hace cada gesto según la fase. Sin ventana, como siempre.
// ---------------------------------------------------------------------------

/// En qué está el diálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Capture {
    #[default]
    None,
    /// Esperando una tecla del teclado para la fila con foco.
    Key,
    /// Esperando un botón del mando para la fila con foco.
    Button,
}

#[derive(Debug, Default)]
pub struct Dialog {
    pub is_open: bool,
    pub row: i32,
    pub capture: Capture,
}

impl Dialog {
    pub fn open(&mut self) {
        self.is_open = true;
        self.row = 0;
        self.capture = Capture::None;
    }

    pub fn move_by(&mut self, d: i32) {
        if self.capture == Capture::None {
            self.row = (self.row + d).clamp(0, ACTIONS.len() as i32 - 1);
        }
    }

    pub fn focused_id(&self) -> &'static str {
        ACTIONS[self.row as usize].id
    }

    /// A: empezar a capturar la tecla de la fila.
    pub fn capture_key(&mut self) {
        if self.capture == Capture::None {
            self.capture = Capture::Key;
        }
    }

    /// X: empezar a capturar el botón — salvo en una dirección, que no tiene.
    pub fn capture_button(&mut self) {
        if self.capture == Capture::None && ACTIONS[self.row as usize].default_button.is_some() {
            self.capture = Capture::Button;
        }
    }

    /// Llegó una tecla del teclado. En captura, asigna (si es asignable) y
    /// termina; fuera de captura no hace nada — las teclas de navegación las
    /// maneja la UI.
    pub fn key_received(&mut self, c: &mut Controls, text: &str) -> bool {
        if self.capture != Capture::Key {
            return false;
        }
        let Some(name) = capture_key_name(text) else {
            return false;
        };
        c.assign_key(self.focused_id(), &name);
        self.capture = Capture::None;
        true
    }

    /// Llegó un botón crudo del mando (nombre SDL, del hilo de gilrs).
    pub fn button_received(&mut self, c: &mut Controls, button: &str) -> bool {
        if self.capture != Capture::Button {
            return false;
        }
        c.assign_button(self.focused_id(), button);
        self.capture = Capture::None;
        true
    }

    /// B: cancela la captura si hay una; si no, cierra. Devuelve `true` si el
    /// diálogo quedó cerrado — dos escalones a propósito: quien se arrepintió
    /// de capturar no quería irse de la pantalla.
    pub fn close(&mut self) -> bool {
        if self.capture != Capture::None {
            self.capture = Capture::None;
            return false;
        }
        self.is_open = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EL CONTRATO, COMPARADO Y NO PROMETIDO: las claves y los defaults de acá
    /// son los de Runtime `src/input_map.cpp`. Dos copias
    /// que nadie compara son dos contratos, y el día que difieren el jugador
    /// remapea «a» y en el juego cambia otra cosa.
    #[test]
    fn the_keys_and_defaults_are_the_runtime_ones() {
        let runtime_root = crate::runtime_source::root();
        let input_map_path = runtime_root.join("src/input_map.cpp");
        let input_map_source = std::fs::read_to_string(&input_map_path).unwrap_or_else(|error| {
            panic!(
                "el Runtime no expone el contrato de controles en {}: {error}",
                input_map_path.display()
            )
        });
        for action in &ACTIONS {
            assert!(
                input_map_source.contains(&format!("return \"{}\";", action.id)),
                "el runtime no conoce la clave «{}»",
                action.id
            );
        }

        // Los defaults de teclado, por scancode; los de botón, por nombre SDL.
        for (id, scan_code) in [
            ("up", "SDL_SCANCODE_UP"),
            ("b", "SDL_SCANCODE_Z"),
            ("a", "SDL_SCANCODE_X"),
            ("start", "SDL_SCANCODE_RETURN"),
            ("select", "SDL_SCANCODE_RSHIFT"),
        ] {
            let _ = id;
            assert!(
                input_map_source.contains(scan_code),
                "el default de fábrica cambió: falta {scan_code}"
            );
        }
        assert!(
            input_map_source.contains("SDL_GAMEPAD_BUTTON_SOUTH"),
            "el botón de fábrica de `b` ya no es el sur"
        );
    }

    /// EL CONFLICTO INTERCAMBIA: ni acciones mudas ni teclas repetidas, nunca.
    #[test]
    fn assigning_a_taken_key_swaps() {
        let mut c = Controls::default();
        // `b` es Z y `a` es X de fábrica. Saltar («b») quiere la X…
        c.assign_key("b", "X");
        assert_eq!(c.key_of("b"), "X");
        // …y «a» no queda muda: se lleva la Z que «b» soltó.
        assert_eq!(c.key_of("a"), "Z");

        // Nunca hay dos acciones con la misma tecla.
        let mut seen_inputs = std::collections::BTreeSet::new();
        for a in &ACTIONS {
            assert!(
                seen_inputs.insert(c.key_of(a.id)),
                "dos acciones comparten la tecla {}",
                c.key_of(a.id)
            );
        }
    }

    /// Y con los botones igual — pero las direcciones no juegan: ni reciben
    /// botón ni se lo pueden robar.
    #[test]
    fn buttons_swap_and_directions_stay_out() {
        let mut c = Controls::default();
        c.assign_button("b", "north"); // el botón de «x»
        assert_eq!(c.button_of("b").as_deref(), Some("north"));
        assert_eq!(c.button_of("x").as_deref(), Some("south"));

        assert_eq!(c.button_of("up"), None, "una dirección no tiene botón");
        c.assign_button("up", "south");
        assert_eq!(c.button_of("up"), None, "y no puede recibir uno");
    }

    /// Volver una acción a su tecla de fábrica LIMPIA el override: el TOML de
    /// quien deshizo sus cambios vuelve a no hacer falta.
    #[test]
    fn factory_reset_clears_the_override() {
        let mut c = Controls::default();
        c.assign_key("b", "K");
        assert!(c.has_changes());
        c.assign_key("b", "Z");
        assert!(!c.has_changes(), "volver al default no puede dejar residuo");

        c.assign_key("start", "Space");
        c.restore();
        assert!(!c.has_changes());
        assert_eq!(c.key_of("start"), "Return");
    }

    /// El TOML es COMPLETO y estable: las 12 teclas, los 8 botones, y el mismo
    /// texto para el mismo estado — es un archivo que se versiona a ojo.
    #[test]
    fn the_toml_carries_the_whole_map() {
        let mut c = Controls::default();
        c.assign_key("a", "K");
        let t = c.toml();
        assert!(t.contains("[keyboard]") && t.contains("[gamepad]"));
        assert!(t.contains("a = \"K\""), "lo elegido va");
        assert!(
            t.contains("b = \"Z\""),
            "lo de fábrica también: el mapa entero"
        );
        assert!(t.contains("select = \"Right Shift\""));
        assert!(
            t.contains("start = \"start\""),
            "la sección [gamepad] va completa"
        );
        assert!(
            !t.contains("up = \"dpup\""),
            "las direcciones no llevan botón"
        );
        assert_eq!(t, c.toml(), "el mismo estado produce el mismo archivo");
    }

    /// La captura entiende lo que Slint manda: letras a mayúscula, la barra a
    /// «Space», los nombres especiales tal cual — y Escape NO asigna, porque es
    /// cómo se cancela.
    #[test]
    fn capture_normalizes_what_slint_sends() {
        assert_eq!(capture_key_name("k").as_deref(), Some("K"));
        assert_eq!(capture_key_name("K").as_deref(), Some("K"));
        assert_eq!(capture_key_name("7").as_deref(), Some("7"));
        assert_eq!(capture_key_name(" ").as_deref(), Some("Space"));
        assert_eq!(capture_key_name(",").as_deref(), Some(","));
        assert_eq!(capture_key_name("Up").as_deref(), Some("Up"));
        assert_eq!(
            capture_key_name("Right Shift").as_deref(),
            Some("Right Shift")
        );
        assert_eq!(
            capture_key_name("Escape"),
            None,
            "Escape cancela, no asigna"
        );
        assert_eq!(capture_key_name("F5"), None, "las F son del chasis");
    }

    /// El diálogo: A captura tecla, X captura botón (salvo direcciones), y B
    /// tiene dos escalones — cancela la captura primero, cierra después.
    #[test]
    fn the_dialog_captures_and_b_has_two_steps() {
        let mut d = Dialog::default();
        let mut c = Controls::default();
        d.open();

        // La fila 4 es «b» (después de las cuatro direcciones).
        d.row = 4;
        d.capture_key();
        assert_eq!(d.capture, Capture::Key);
        assert!(
            !d.moving_is_possible(),
            "capturando, la cruceta no mueve el foco"
        );
        assert!(d.key_received(&mut c, "j"));
        assert_eq!(c.key_of("b"), "J");
        assert_eq!(d.capture, Capture::None);

        // X sobre una dirección no captura: no hay botón que asignar.
        d.row = 0;
        d.capture_button();
        assert_eq!(d.capture, Capture::None);
        d.row = 4;
        d.capture_button();
        assert!(d.button_received(&mut c, "west"));
        assert_eq!(c.button_of("b").as_deref(), Some("west"));

        d.capture_key();
        assert!(!d.close(), "B con captura abierta la cancela, no cierra");
        assert!(d.is_open);
        assert!(d.close(), "y el segundo B sí cierra");
    }

    impl Dialog {
        /// Sólo para el test de arriba: la regla que `mover` aplica.
        fn moving_is_possible(&self) -> bool {
            self.capture == Capture::None
        }
    }
}
