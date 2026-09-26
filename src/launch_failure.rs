//! Preserve startup failures and turn Runtime diagnostics into actionable text.
use crate::runtime_event::RuntimeEvent;

#[derive(Debug, Default, Clone)]
pub struct Failure {
    pub reason: String,
    pub detail: String,
    pub started: bool,
    pub explicit_error: bool,
}

pub struct Notice {
    pub title: String,
    pub explanation: String,
    pub pack_rejected: bool,
}

impl Failure {
    pub fn observe(&mut self, event: &RuntimeEvent) {
        match event {
            RuntimeEvent::Ready { .. } => self.started = true,
            RuntimeEvent::Error { reason, message } => {
                self.explicit_error = true;
                self.reason.clone_from(reason);
                self.detail = bounded(message);
            }
            RuntimeEvent::Warning { reason, message }
                if !self.explicit_error
                    && matches!(
                        reason.as_str(),
                        "pack.open_failed"
                            | "pack.trust_registry_invalid"
                            | "input.map_invalid"
                            | "core.load_failed"
                            | "core.invalid"
                            | "cli.invalid_argument"
                            | "protocol.incompatible"
                    ) =>
            {
                self.reason.clone_from(reason);
                self.detail = bounded(message);
            }
            _ => {}
        }
    }

    pub fn finish(&mut self, exit_code: Option<i32>, stderr: &str) {
        if self.reason.is_empty() {
            self.reason = match exit_code {
                Some(2) => "core.load_failed",
                Some(3) => "core.invalid",
                Some(4) => "pack.open_failed",
                Some(65) => "protocol.incompatible",
                _ => "",
            }
            .into();
        }
        if !stderr.is_empty() {
            self.detail.push('\n');
            self.detail.push_str(&bounded(stderr));
        }
        self.detail = crate::logging::redact(&self.detail);
    }

    pub fn notice(&self) -> Notice {
        let pack_rejected = matches!(
            self.reason.as_str(),
            "pack.open_failed" | "pack.trust_registry_invalid"
        );
        let (title, explanation) = match self.reason.as_str() {
            "pack.open_failed"
                if self.detail.contains("Trust(UnknownKey(")
                    || self.detail.contains("unknown signing key '") =>
            {
                (
                    tr!("No se reconoce la firma del pack"),
                    tr!(
                        "El pack está firmado, pero falta registrar su certificado de confianza en este equipo. Podés jugar sin el pack o configurar el certificado público de su autor."
                    ),
                )
            }
            "pack.open_failed" => (
                tr!("No se pudo cargar el pack"),
                tr!(
                    "El sistema rechazó el pack al comprobarlo. Volvé a instalarlo desde una copia válida o elegí otro. También podés jugar al original sin el pack."
                ),
            ),
            "pack.trust_registry_invalid" => (
                tr!("No se pudo leer el certificado del pack"),
                tr!(
                    "El archivo de certificados configurado no existe, no se puede leer o tiene un formato inválido. Revisá su ubicación. Podés jugar sin el pack mientras lo corregís."
                ),
            ),
            "input.map_invalid" => (
                tr!("No se pudieron cargar los controles"),
                tr!("El mapa de controles no es válido. Revisá Ajustes > Controles del juego."),
            ),
            "core.load_failed" | "core.invalid" => (
                tr!("No se pudo cargar el core"),
                tr!(
                    "El emulador elegido no se pudo abrir. Revisá el core de esta consola en Ajustes y comprobá que sus archivos estén disponibles."
                ),
            ),
            "runtime.spawn_failed" | "protocol.incompatible" => (
                tr!("No se pudo iniciar el juego"),
                tr!(
                    "No se pudo iniciar el motor de juego. Revisá que AYTHER Runtime esté instalado y sea compatible con esta versión de Play."
                ),
            ),
            _ if !self.started => (
                tr!("No se pudo iniciar el juego"),
                tr!(
                    "El juego no llegó a arrancar. El diagnóstico contiene los detalles del fallo. Podés probar el modo seguro, que desactiva las mejoras."
                ),
            ),
            _ => (
                tr!("EL JUEGO SE CERRÓ INESPERADAMENTE"),
                tr!(
                    "La partida se interrumpió. Podés intentar reiniciarla en modo seguro; la ROM y los guardados existentes se conservan."
                ),
            ),
        };
        Notice {
            title,
            explanation,
            pack_rejected,
        }
    }

    pub fn diagnostic(&self) -> String {
        crate::logging::redact(&format!(
            "runtime_reason = {}\npartida_iniciada = {}\n{}\n",
            self.reason, self.started, self.detail
        ))
    }
}

fn bounded(text: &str) -> String {
    let mut end = text.len().min(8192);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_pack_rejection_keeps_the_reason_and_offers_playing_without_it() {
        let mut failure = Failure::default();
        failure.observe(&RuntimeEvent::Warning {
            reason: "pack.open_failed".into(),
            message: "cannot open pack".into(),
        });
        failure.finish(Some(4), "[AyArchive] Trust(UnknownKey(\"local-author\"))");
        let notice = failure.notice();
        assert_eq!(failure.reason, "pack.open_failed");
        assert!(notice.title.contains("firma"));
        assert!(notice.explanation.contains("certificado"));
        assert!(notice.pack_rejected);
    }

    #[test]
    fn ordinary_warnings_do_not_turn_into_failures_or_claim_a_bad_signature() {
        let mut failure = Failure::default();
        failure.observe(&RuntimeEvent::Warning {
            reason: "pack.no_active_subsystems".into(),
            message: "warning".into(),
        });
        assert!(failure.reason.is_empty());
        assert!(!failure.explicit_error);
        failure.finish(Some(4), "invalid archive");
        assert!(!failure.notice().title.contains("firma"));
    }

    #[test]
    fn failure_before_ready_differs_from_an_interrupted_game() {
        let mut failure = Failure::default();
        assert!(failure.notice().title.contains("iniciar"));
        failure.observe(&RuntimeEvent::Ready {
            game_id: "game".into(),
            has_pack: 0,
            manifest: String::new(),
        });
        assert!(failure.notice().title.contains("CERRÓ"));
    }

    #[test]
    fn technical_details_are_redacted_and_bounded() {
        let mut failure = Failure::default();
        failure.finish(
            Some(4),
            &format!("C:\\Users\\someone\\secret\\game.md {}", "á".repeat(20000)),
        );
        let report = failure.diagnostic();
        assert!(!report.contains("someone"));
        assert!(!report.contains("secret"));
        assert!(report.len() < 9000);
    }
}
