//! feedback.rs — la opinión del jugador sobre un pack, después de jugarlo (#313).
//!
//! Es la mitad del ciclo que mira hacia el autor: el pack se instala, se juega
//! y alguien tiene que poder decir si la remasterización quedó bien.
//!
//! ## En CE es local, y se acaba ahí
//!
//! En AYTHER Play el formulario se guarda primero en el disco y el jugador
//! decide si lo manda; acá no hay a quién mandarlo (ADR-005), así que **lo
//! guardado es lo que hay**: una nota del jugador para sí mismo, en su carpeta
//! de datos, que puede leer, copiar y hacerle llegar al autor por donde quiera.
//!
//! Por eso no hay un campo `sent` ni una cola de envío. Un `bool` que nunca va
//! a ser `true` no es un estado: es una promesa esperando a alguien que la
//! cumpla, y el que abre el archivo la lee como una función rota.
//!
//! ## Lo que se guarda y lo que NO
//!
//! Van las siete dimensiones, el id del pack y su versión, y cuánto se jugó.
//! **No van la ROM, ni su ruta, ni savestates, ni nada del disco.** El pack se
//! identifica por su id, que es lo que el autor publicó, y el juego por su
//! `game_id` simbólico. Un test lo verifica sobre el archivo serializado, no
//! sobre la intención: lo que no está en la struct no se puede filtrar, pero lo
//! que se serializa se puede leer.
//!
//! ## No preguntar de más
//!
//! El riesgo anotado en la issue es la fatiga. Tres reglas, todas del lado del
//! jugador:
//!
//!   · **uso mínimo** — no se pregunta por una sesión de dos minutos: no hay
//!     opinión formada, y preguntar igual enseña a ignorar el formulario;
//!   · **una vez por versión del pack** — ya opinó; si el autor publica otra
//!     versión, esa es una pregunta nueva;
//!   · **«ahora no» tiene memoria** — se espera a la próxima sesión, y
//!     **«no preguntar más» es permanente** y por jugador, no por pack.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Las siete dimensiones de la issue, en el orden en que se muestran.
///
/// Son las que distinguen un pack de otro, no una estrella global: «me gustó»
/// no le dice al autor qué mejorar, y «el balance música/efectos está bajo» sí.
pub const DIMENSIONS: [&str; 7] = [
    "Fidelidad al original",
    "Calidad visual",
    "Claridad del audio",
    "Balance música/efectos",
    "Rendimiento",
    "Estabilidad",
    "Preferencia vs. original",
];

/// Cuánto hay que haber jugado para que la pregunta tenga sentido.
pub const MIN_PLAYED_SECS: u64 = 10 * 60;

/// Una opinión, tal como se guarda.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Feedback {
    /// Id del pack (lo que el autor publicó), no una ruta del disco.
    pub pack_id: String,
    /// Versión del pack sobre la que se opinó: una opinión sobre la 1.0 no
    /// habla de la 1.1.
    pub pack_version: String,
    /// Id simbólico del juego (`game_id` del manifest). Nunca la ROM.
    pub game_id: String,
    /// Minutos jugados, redondeados. En minutos y no en segundos porque es lo
    /// que hace falta para saber si la opinión está formada, y menos precisión
    /// es menos dato sobre la persona.
    pub played_min: u64,
    /// 1..5 por dimensión; 0 = no contestada (se puede mandar a medias).
    pub ratings: Vec<u8>,
    /// Comentario libre, opcional.
    #[serde(default)]
    pub comment: String,
}

impl Feedback {
    pub fn new(pack_id: &str, pack_version: &str, game_id: &str, played_secs: u64) -> Self {
        Feedback {
            pack_id: pack_id.to_string(),
            pack_version: pack_version.to_string(),
            game_id: game_id.to_string(),
            played_min: played_secs / 60,
            ratings: vec![0; DIMENSIONS.len()],
            comment: String::new(),
        }
    }
    /// ¿Contestó algo? Un formulario en blanco no es una opinión y no se guarda.
    pub fn answered(&self) -> bool {
        self.ratings.iter().any(|r| *r > 0) || !self.comment.is_empty()
    }
}

/// Lo que el jugador decidió sobre la pregunta en sí.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackState {
    /// «No preguntar más»: permanente y para todos los packs.
    #[serde(default)]
    pub never_ask: bool,
    /// Packs sobre los que ya opinó o pospuso: `"<pack_id>@<version>"`.
    #[serde(default)]
    pub answered: Vec<String>,
    #[serde(default)]
    pub postponed: Vec<String>,
}

fn key(pack_id: &str, version: &str) -> String {
    format!("{pack_id}@{version}")
}

impl FeedbackState {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("feedback_state.json")
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read(Self::path(dir))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::path(dir), text).map_err(|e| e.to_string())
    }

    /// ¿Se pregunta después de esta sesión?
    ///
    /// Sin pack no se pregunta: la opinión es sobre la remasterización, y sin
    /// pack lo que se jugó es el juego original.
    pub fn should_ask(&self, pack_id: &str, version: &str, played_secs: u64) -> bool {
        if self.never_ask || pack_id.is_empty() {
            return false;
        }
        if played_secs < MIN_PLAYED_SECS {
            return false;
        }
        let k = key(pack_id, version);
        !self.answered.contains(&k)
    }

    /// «Ahora no»: se recuerda, pero no cierra la puerta — la próxima sesión
    /// larga vuelve a preguntar.
    pub fn postpone(&mut self, pack_id: &str, version: &str) {
        let k = key(pack_id, version);
        if !self.postponed.contains(&k) {
            self.postponed.push(k);
        }
    }

    pub fn mark_answered(&mut self, pack_id: &str, version: &str) {
        let k = key(pack_id, version);
        if !self.answered.contains(&k) {
            self.answered.push(k);
        }
    }
}

/// Guarda una opinión en el disco del jugador. Un archivo por pack y versión:
/// re-opinar sobre la misma versión pisa lo anterior (es la misma opinión,
/// corregida), y opinar sobre otra versión es otro archivo.
pub fn save(dir: &Path, fb: &Feedback) -> Result<PathBuf, String> {
    if !fb.answered() {
        return Err("el formulario está en blanco".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let safe: String = fb
        .pack_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe_version: String = fb
        .pack_version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let file = dir.join(format!("{safe}@{safe_version}.json"));
    let text = serde_json::to_vec_pretty(fb).map_err(|e| e.to_string())?;
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Las opiniones guardadas, para que el jugador pueda ver qué escribió.
///
/// En AYTHER Play esto es la cola de envío. Acá no hay envío, así que devuelve
/// TODO lo guardado — que es lo mismo, porque nada se marca nunca como ido. La
/// pantalla que las lista todavía no existe; hasta entonces lo que fija el
/// formato del archivo son sus oráculos, y el `allow` no cubre `cargo test`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn saved_feedback(dir: &Path) -> Vec<Feedback> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        if p.file_name().and_then(|x| x.to_str()) == Some("feedback_state.json") {
            continue;
        }
        if let Some(fb) = std::fs::read(&p)
            .ok()
            .and_then(|b| serde_json::from_slice::<Feedback>(&b).ok())
        {
            out.push(fb);
        }
    }
    out.sort_by(|a, b| a.pack_id.cmp(&b.pack_id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fb() -> Feedback {
        let mut f = Feedback::new("ga-hd", "1.0", "goldenaxe", 45 * 60);
        f.ratings = vec![5, 4, 5, 3, 5, 5, 4];
        f
    }

    /// Lo que se guarda NO lleva la ROM, ni rutas, ni savestates. Se verifica
    /// sobre el archivo SERIALIZADO y no sobre la struct: lo que no está en la
    /// struct no se puede filtrar, pero lo que se serializa alguien lo lee.
    #[test]
    fn nothing_private_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let file = save(dir.path(), &fb()).unwrap();
        let text = std::fs::read_to_string(&file).unwrap().to_lowercase();

        for forbidden in [
            ".md",
            ".bin",
            ".srm",
            ".state",
            "c:/",
            "c:\\",
            "users",
            "rom",
            "savestate",
        ] {
            assert!(
                !text.contains(forbidden),
                "el feedback guardado contiene «{forbidden}»: {text}"
            );
        }
        // Y sí lleva lo que el autor necesita para entenderlo.
        assert!(text.contains("ga-hd") && text.contains("goldenaxe") && text.contains("45"));
    }

    /// El riesgo de la issue es la fatiga de encuestas. Las tres reglas.
    #[test]
    fn it_does_not_ask_too_much() {
        let mut st = FeedbackState::default();

        // Uso mínimo: dos minutos no forman una opinión.
        assert!(!st.should_ask("ga-hd", "1.0", 2 * 60));
        assert!(st.should_ask("ga-hd", "1.0", 45 * 60));

        // Sin pack no hay nada que opinar: se jugó el juego original.
        assert!(!st.should_ask("", "", 45 * 60));

        // Una vez por versión: ya opinó sobre la 1.0…
        st.mark_answered("ga-hd", "1.0");
        assert!(!st.should_ask("ga-hd", "1.0", 45 * 60));
        // …pero la 1.1 es una pregunta nueva.
        assert!(st.should_ask("ga-hd", "1.1", 45 * 60));

        // «Ahora no» no cierra la puerta: la próxima sesión larga pregunta.
        st.postpone("ga-hd", "1.1");
        assert!(st.should_ask("ga-hd", "1.1", 45 * 60));

        // «No preguntar más» es permanente y para TODOS los packs.
        st.never_ask = true;
        assert!(!st.should_ask("ga-hd", "1.1", 45 * 60));
        assert!(!st.should_ask("otro", "2.0", 90 * 60));
    }

    /// La decisión sobrevive al cierre del launcher — si no, «no preguntar más»
    /// sería «no preguntar hasta la próxima vez que abras».
    /// #602 — EL CA: una opinión por PACK Y VERSIÓN, no por pack.
    ///
    /// La maquinaria ya distinguía las dos cosas; lo que faltaba era la
    /// versión. Play mandaba `""` desde `main.rs`, así que la clave era sólo el
    /// pack: quien opinaba sobre la 1.0 no volvía a ser preguntado nunca, y el
    /// autor que arreglaba su pack no tenía cómo saber si había mejorado. Este
    /// test fija el contrato que ese `""` estaba rompiendo desde afuera.
    #[test]
    fn one_opinion_per_pack_version_and_not_per_pack() {
        let mut st = FeedbackState::default();
        st.mark_answered("golden-axe-hd", "1.0");

        assert!(
            !st.should_ask("golden-axe-hd", "1.0", 3600),
            "ya opino sobre esta version"
        );
        assert!(
            st.should_ask("golden-axe-hd", "1.1", 3600),
            "el pack cambio: es otra cosa sobre la que opinar"
        );
        assert!(st.should_ask("otro-pack", "1.0", 3600));

        // Y el caso que el defecto producía: con la versión vacía, TODAS las
        // versiones comparten clave y sólo se pregunta una vez en la vida.
        let mut empty_state = FeedbackState::default();
        empty_state.mark_answered("golden-axe-hd", "");
        assert!(!empty_state.should_ask("golden-axe-hd", "", 3600));
        assert!(
            empty_state.should_ask("golden-axe-hd", "1.0", 3600),
            "una version real no puede quedar tapada por la clave vacia"
        );
    }

    #[test]
    fn the_decision_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = FeedbackState {
            never_ask: true,
            ..Default::default()
        };
        st.mark_answered("ga-hd", "1.0");
        st.save(dir.path()).unwrap();

        let read_back = FeedbackState::load(dir.path());
        assert!(read_back.never_ask);
        assert!(!read_back.should_ask("ga-hd", "1.0", 90 * 60));
    }

    /// Un formulario en blanco no es una opinión: no se guarda, y decirlo es
    /// mejor que dejar un archivo vacío que después alguien cuenta como voto.
    #[test]
    fn a_blank_form_is_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let blank_feedback = Feedback::new("ga-hd", "1.0", "goldenaxe", 45 * 60);
        assert!(!blank_feedback.answered());
        assert!(save(dir.path(), &blank_feedback).is_err());
        assert!(saved_feedback(dir.path()).is_empty());
    }

    /// Guardar y volver a leer no pasa por ninguna red — en CE no hay ninguna
    /// que pasar—, y re-opinar sobre la misma versión CORRIGE en vez de
    /// acumular: dos archivos de la misma versión serían dos votos de uno.
    #[test]
    fn what_was_saved_reads_back_and_opining_again_amends() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &fb()).unwrap();

        let p = saved_feedback(dir.path());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].pack_id, "ga-hd");

        // Re-opinar sobre la MISMA versión corrige, no acumula.
        let mut revised_feedback = fb();
        revised_feedback.ratings[0] = 2;
        save(dir.path(), &revised_feedback).unwrap();
        let p = saved_feedback(dir.path());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].ratings[0], 2);

        // Otra versión es otra opinión.
        let mut v11 = fb();
        v11.pack_version = "1.1".into();
        save(dir.path(), &v11).unwrap();
        assert_eq!(saved_feedback(dir.path()).len(), 2);
    }

    /// El estado vive en el mismo directorio y no se cuenta como una opinión.
    #[test]
    fn the_state_is_not_read_as_feedback() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = FeedbackState::default();
        st.mark_answered("ga-hd", "1.0");
        st.save(dir.path()).unwrap();
        assert!(saved_feedback(dir.path()).is_empty());
    }
}
