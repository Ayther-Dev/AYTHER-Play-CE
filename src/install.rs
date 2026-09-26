//! install.rs — un pack entra desde el disco, con el mando (#678).
//!
//! Es LA pantalla de packs de CE, y por eso existe: en AYTHER Play los packs
//! llegan del catálogo del Hub; acá el jugador ya tiene el `.ay` —se lo pasó
//! un amigo, lo horneó con `ay_pack`, lo bajó de donde quiso— y lo único que
//! falta es que entrar no exija teclado ni línea de comandos. El launcher se
//! usa desde el sillón (§6).
//!
//! # Una pantalla, dos elecciones
//!
//! El mismo diálogo sirve para las dos decisiones de #678, porque son la misma
//! operación con distinto final:
//!
//! * **`Modo::Archivo`** — elegir un `.ay` e instalarlo. Termina en
//!   `pack_install::install_local`, que audita ANTES de tocar lo instalado y
//!   conserva la versión anterior.
//! * **`Modo::Carpeta`** — designar la carpeta de packs. No instala nada: lo
//!   que aparezca ahí queda disponible, como una ROM en la carpeta de ROMs.
//!
//! Dos diálogos separados serían dos maneras de navegar el disco, y el wizard
//! ya pagó ese precio una vez (#588): por eso la lista, el filtro y el gesto
//! de «usar esta carpeta» son los mismos de `explorer.rs`.
//!
//! # Lo que decide vive acá, no en el callback
//!
//! Igual que `pad_router` y `onboarding`: la máquina —qué hace A sobre una
//! fila, qué dice el resultado, cuándo se ofrece revertir— es una función pura
//! y se prueba sin abrir una ventana. `main.rs` sólo traduce a Slint.

use crate::explorer::{self, Entry, Search};
use crate::pack_install::InstallResult;
use std::path::{Path, PathBuf};

/// Qué se está eligiendo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Un `.ay` para instalar.
    File,
    /// La carpeta de packs del jugador.
    Folder,
    /// Una imagen para la carátula de un juego.
    Cover,
}

/// En qué está el diálogo.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    /// Navegando el disco.
    Browsing,
    /// El pack elegido se está auditando e instalando, en su hilo. La pantalla
    /// lo dice: sobre un `.ay` de cientos de MB la auditoría se siente.
    Working,
    /// Terminó, bien o mal, y el resultado está en pantalla.
    Outcome(Outcome),
}

/// Lo que la pantalla dice al final. Se arma en [`resultado_de`], que es pura:
/// el texto que promete «la anterior quedó guardada» se prueba contra el
/// `InstallResult` real, no contra la intención.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub title: String,
    pub detail: String,
    /// Hay una versión anterior guardada y «Y Revertir» hace algo. Sólo puede
    /// ser `true` cuando la instalación REEMPLAZÓ: ofrecer revertir una primera
    /// instalación sería un botón que sólo contesta «no hay a dónde».
    pub can_revert: bool,
    /// El id instalado, para revertir sin volver a deducirlo del nombre.
    pub pack_id: String,
}

pub struct State {
    pub is_open: bool,
    pub mode: Mode,
    pub phase: Phase,
    pub row: i32,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    /// Ruta estable de la ROM; los índices de la vista pueden cambiar al escanear.
    pub cover_game: Option<String>,
    pub cover_error: String,
}

impl Default for State {
    fn default() -> Self {
        State {
            is_open: false,
            mode: Mode::File,
            phase: Phase::Browsing,
            row: 0,
            dir: PathBuf::new(),
            entries: Vec::new(),
            cover_game: None,
            cover_error: String::new(),
        }
    }
}

/// La extensión que este diálogo busca. `ay` y nada más: mostrar los `.zip` «por
/// si acaso» sería invitar a instalar algo que el instalador va a rechazar.
const EXT_PACK: &[&str] = &["ay"];

impl State {
    /// Abre el diálogo en el modo pedido, arrancando por donde tenga sentido:
    /// donde el jugador se quedó la última vez si el diálogo ya navegó, o
    /// `desde` (la carpeta de packs actual, la home) si no.
    pub fn open(&mut self, mode: Mode, source: PathBuf) {
        let start = if self.dir.exists() && self.mode == mode {
            self.dir.clone()
        } else if source.exists() {
            source
        } else {
            explorer::starting_path()
        };
        self.mode = mode;
        self.cover_game = None;
        self.cover_error.clear();
        self.phase = Phase::Browsing;
        self.row = 0;
        self.navigate_to(start);
        self.is_open = true;
    }

    fn search(&self) -> Search {
        match self.mode {
            Mode::File => Search::File(EXT_PACK),
            Mode::Folder => Search::Folder,
            Mode::Cover => Search::File(&["png", "jpg", "jpeg", "webp"]),
        }
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        self.entries = explorer::list_entries(&dir, self.search());
        self.dir = dir;
        self.row = 0;
    }

    /// Arriba/abajo. Con tope y sin envolver, como todas las listas del chasis.
    pub fn move_by(&mut self, d: i32) {
        let n = self.entries.len() as i32;
        if n > 0 {
            self.row = (self.row + d).clamp(0, n - 1);
        }
    }

    /// «A» sobre la fila enfocada: entra en la carpeta, o —en modo archivo—
    /// devuelve el `.ay` elegido para que el llamador lo instale en su hilo.
    ///
    /// Sólo navega mientras no se esté [`Fase::Explorando`]: un A que llega
    /// tarde, con la instalación ya corriendo, no puede elegir otro archivo.
    pub fn activate(&mut self) -> Option<PathBuf> {
        if self.phase != Phase::Browsing {
            return None;
        }
        let e = self.entries.get(self.row as usize)?.clone();
        if e.is_dir {
            self.navigate_to(e.path);
            None
        } else {
            self.phase = Phase::Working;
            Some(e.path)
        }
    }

    /// «X» en modo carpeta: designar la carpeta que se está MIRANDO. Estando
    /// adentro y no señalándola desde afuera — la misma regla del wizard, y por
    /// el mismo motivo: evita la duda de si la fila seleccionada significa esa
    /// carpeta o la que la contiene.
    pub fn select_current_folder(&mut self) -> Option<PathBuf> {
        if self.mode != Mode::Folder || self.phase != Phase::Browsing {
            return None;
        }
        self.is_open = false;
        Some(self.dir.clone())
    }

    /// «B». Devuelve `true` si el diálogo quedó cerrado.
    ///
    /// En el resultado, B CIERRA: ya no hay nada que cancelar. Explorando,
    /// cierra también — subir de carpeta es la fila «..», no B, porque B como
    /// «subir» convierte salir del diálogo en una escalera de pulsaciones.
    /// Y trabajando, NO cierra: la instalación no se cancela a medias —el
    /// staging y la promoción atómica son de `pack_install`— y un diálogo que
    /// desaparece mientras trabaja parece que abortó.
    pub fn close(&mut self) -> bool {
        if self.phase == Phase::Working {
            return false;
        }
        self.is_open = false;
        true
    }

    /// El id con el que se instala: el nombre del archivo, que es lo que el
    /// jugador ve y lo que `--rollback <id>` espera. La misma regla que el CLI.
    pub fn id_from(path: &Path) -> String {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "pack".into())
    }
}

/// El resultado, en palabras. PURA: recibe lo que pasó y devuelve lo que se
/// dice, así el texto que promete «la anterior quedó guardada» sólo puede
/// aparecer cuando de verdad la hay.
///
/// `nombre` y `version` son lo que declaró el manifest (leído DESPUÉS de
/// instalar, del pack ya promovido); si el manifest no los trae, el id del
/// archivo es el nombre honesto.
pub fn outcome_of(pack_id: &str, name: &str, version: &str, r: &InstallResult) -> Outcome {
    let name = if name.is_empty() { pack_id } else { name };
    let display_name = if version.is_empty() {
        name.to_string()
    } else {
        format!("{name} {version}")
    };
    match r {
        InstallResult::Installed { replaced: false } => Outcome {
            title: tr!("Instalado: {}", display_name),
            detail: tr!(
                "El pack quedó instalado y auditado: completo y sin alterar. Aparece en el juego que declara, con la biblioteca ya al día."
            ),
            can_revert: false,
            pack_id: pack_id.into(),
        },
        InstallResult::Installed { replaced: true } => Outcome {
            title: tr!("Actualizado: {}", display_name),
            detail: tr!(
                "El pack nuevo quedó instalado y auditado. La versión anterior quedó guardada: podés volver a ella desde acá o cuando quieras."
            ),
            can_revert: true,
            pack_id: pack_id.into(),
        },
        InstallResult::Invalid(reason) => Outcome {
            title: tr!("No se instaló"),
            detail: tr!(
                "El archivo no pasó la auditoría: {}. Lo que tenías instalado sigue intacto.",
                reason
            ),
            can_revert: false,
            pack_id: pack_id.into(),
        },
        InstallResult::Io(e) => Outcome {
            title: tr!("No se pudo instalar"),
            detail: tr!(
                "Falló el disco, no el pack: {}. Lo que tenías instalado sigue intacto.",
                e
            ),
            can_revert: false,
            pack_id: pack_id.into(),
        },
    }
}

/// Lo que se dice después de «Y Revertir».
pub fn revert_outcome_of(pack_id: &str, r: &Result<(), String>) -> Outcome {
    match r {
        Ok(()) => Outcome {
            title: tr!("Se volvió a la versión anterior"),
            detail: tr!(
                "La que estaba quedó guardada a su vez: revertir INTERCAMBIA, no borra, así que se puede volver a adelantar."
            ),
            // Cierto por cómo funciona `rollback`: intercambia `.ay` y `.prev`,
            // así que después de revertir siempre hay a dónde volver.
            can_revert: true,
            pack_id: pack_id.into(),
        },
        Err(e) => Outcome {
            title: tr!("No se pudo revertir"),
            detail: tr!("{}. El pack instalado no se tocó.", e),
            can_revert: false,
            pack_id: pack_id.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("packs-bajados");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("golden.ay"), b"x").unwrap();
        std::fs::write(d.join("leeme.txt"), b"x").unwrap();
        std::fs::write(d.join("sub").join("otro.ay"), b"x").unwrap();
        (t, d)
    }

    /// El diálogo sólo muestra lo que se puede instalar: carpetas y `.ay`. Un
    /// `.txt` en la lista es una fila que al elegirla fracasa.
    #[test]
    fn in_file_mode_only_folders_and_packs_show() {
        let (_t, d) = tree();
        let mut e = State::default();
        e.open(Mode::File, d);
        let names: Vec<&str> = e.entries.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"golden.ay"), "{names:?}");
        assert!(names.contains(&"sub"), "{names:?}");
        assert!(!names.contains(&"leeme.txt"), "{names:?}");
    }

    #[test]
    fn cover_picker_filters_images_and_cancels_without_selecting() {
        let (_t, d) = tree();
        std::fs::write(d.join("cover.JPG"), b"image").unwrap();
        std::fs::write(d.join("cover.webp"), b"image").unwrap();
        let mut e = State::default();
        e.open(Mode::Cover, d);
        let files: Vec<_> = e
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(files, ["cover.JPG", "cover.webp"]);
        assert!(e.select_current_folder().is_none());
        assert!(e.close());
        assert!(!e.is_open);
    }

    #[test]
    fn leaving_cover_mode_clears_the_target_game() {
        let (_t, d) = tree();
        let mut e = State::default();
        e.open(Mode::Cover, d.clone());
        e.cover_game = Some("game.md".into());
        e.cover_error = "invalid image".into();
        e.open(Mode::File, d);
        assert!(e.cover_game.is_none());
        assert!(e.cover_error.is_empty());
    }

    /// A sobre una carpeta entra; sobre un pack bloquea nuevas elecciones hasta el resultado.
    #[test]
    fn picking_a_pack_blocks_the_dialog_until_the_result() {
        let (_t, d) = tree();
        let mut e = State::default();
        e.open(Mode::File, d.clone());

        // Entrar en `sub` no elige nada.
        let folder_row = e.entries.iter().position(|x| x.name == "sub").unwrap();
        e.row = folder_row as i32;
        assert_eq!(e.activate(), None);
        assert!(e.dir.ends_with("sub"));

        let pack_row = e.entries.iter().position(|x| x.name == "otro.ay").unwrap();
        e.row = pack_row as i32;
        let selected = e.activate().expect("un .ay se elige");
        assert!(selected.ends_with("otro.ay"));
        assert_eq!(e.phase, Phase::Working);

        assert_eq!(
            e.activate(),
            None,
            "con la instalación corriendo, A no elige otro"
        );
        assert!(!e.close(), "y B no cierra un diálogo que está trabajando");
        assert!(e.is_open);
    }

    /// En modo carpeta se designa la que se está MIRANDO (X), no la fila
    /// seleccionada: la misma regla del wizard, contra la misma ambigüedad.
    #[test]
    fn the_folder_is_picked_from_inside() {
        let (_t, d) = tree();
        let mut e = State::default();
        e.open(Mode::Folder, d.clone());
        let selected = e
            .select_current_folder()
            .expect("X designa la carpeta actual");
        assert_eq!(selected, d);
        assert!(!e.is_open, "designar cierra el diálogo");

        // Y en modo archivo, X no designa nada: sería instalar una carpeta.
        let mut e = State::default();
        e.open(Mode::File, d);
        assert_eq!(e.select_current_folder(), None);
    }

    /// EL TEXTO NO PROMETE LO QUE NO PASÓ. «La anterior quedó guardada» sólo
    /// aparece cuando la instalación reemplazó, y «Revertir» sólo se ofrece
    /// ahí: un botón que contesta «no hay a dónde» enseña a desconfiar de los
    /// botones.
    #[test]
    fn revert_is_only_offered_when_there_is_somewhere_to_go_back() {
        let first_item = outcome_of(
            "ga",
            "Golden Axe HD",
            "2.1.0",
            &InstallResult::Installed { replaced: false },
        );
        assert!(!first_item.can_revert);
        assert!(first_item.title.contains("Golden Axe HD 2.1.0"));
        assert!(
            !first_item.detail.to_lowercase().contains("anterior"),
            "«{}» habla de una versión anterior que no existe",
            first_item.detail
        );

        let replacement = outcome_of(
            "ga",
            "Golden Axe HD",
            "2.2.0",
            &InstallResult::Installed { replaced: true },
        );
        assert!(replacement.can_revert);
        assert!(replacement.detail.contains("anterior"));
    }

    /// Un rechazo dice el motivo Y que lo instalado sigue intacto — que es la
    /// garantía de `install_local` y la única razón para animarse a probar un
    /// `.ay` de origen dudoso.
    #[test]
    fn a_rejection_says_the_reason_and_that_nothing_was_touched() {
        let r = outcome_of(
            "ga",
            "",
            "",
            &InstallResult::Invalid("falta assets/a.png".into()),
        );
        assert!(!r.can_revert);
        assert!(r.detail.contains("falta assets/a.png"));
        assert!(r.detail.contains("sigue intacto"));

        let io = outcome_of("ga", "", "", &InstallResult::Io("disco lleno".into()));
        assert!(io.detail.contains("disco lleno"));
        assert!(io.detail.contains("sigue intacto"));
    }

    /// Sin nombre en el manifest, el título cae al id del archivo: un
    /// «Instalado: » vacío es un resultado que no dice qué pasó.
    #[test]
    fn without_a_manifest_the_title_falls_back_to_the_file_name() {
        let r = outcome_of(
            "mi-pack",
            "",
            "",
            &InstallResult::Installed { replaced: false },
        );
        assert!(r.title.contains("mi-pack"), "{}", r.title);
        assert_eq!(
            State::id_from(Path::new("C:/bajadas/mi-pack.ay")),
            "mi-pack"
        );
    }

    /// Revertir intercambia, y el texto lo dice: después de volver atrás
    /// siempre se puede volver a adelantar, y el botón sigue vivo.
    #[test]
    fn after_reverting_one_can_go_forward_again() {
        let r = revert_outcome_of("ga", &Ok(()));
        assert!(
            r.can_revert,
            "revertir intercambia: siempre hay a dónde volver"
        );
        assert!(r.detail.to_lowercase().contains("no borra"));

        let e = revert_outcome_of("ga", &Err("no hay version anterior".into()));
        assert!(!e.can_revert);
        assert!(e.detail.contains("no se tocó"));
    }

    fn write_pack(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        z.start_file("manifest.toml", o).unwrap();
        write!(
            z,
            "[pack]
name = \"Golden Axe HD\"
version = \"1.0.0\"
"
        )
        .unwrap();
        write!(
            z,
            "game_id = \"ga\"
ayther_min = \"0.1.0\"
schema = 2

"
        )
        .unwrap();
        write!(
            z,
            "[regions]
default = \"NTSC\"
supported = [\"NTSC\"]
"
        )
        .unwrap();
        z.finish().unwrap();
        p
    }

    /// EL CA DE #678, DE PUNTA A PUNTA Y SIN VENTANA: navegar, elegir el `.ay`,
    /// instalarlo con el MISMO camino del CLI, y que el resultado diga la
    /// verdad. Es el recorrido que la pantalla cablea; lo que ella agrega es
    /// sólo el dibujo.
    #[test]
    fn the_whole_journey_pick_install_and_revert() {
        let t = tempfile::tempdir().unwrap();
        let downloads = t.path().join("bajadas");
        let packs = t.path().join("instalados");
        write_pack(&downloads, "ga.ay");

        let mut e = State::default();
        e.open(Mode::File, downloads.clone());
        e.row = e.entries.iter().position(|x| x.name == "ga.ay").unwrap() as i32;
        let path = e.activate().unwrap();

        let id = State::id_from(&path);
        let r = crate::pack_install::install_local(&packs, &id, &path);
        let info = crate::pack_meta::read(&crate::pack_install::installed_path(&packs, &id)).1;
        let res = outcome_of(
            &id,
            info.as_ref().map(|i| i.name.as_str()).unwrap_or(""),
            info.as_ref().map(|i| i.version.as_str()).unwrap_or(""),
            &r,
        );
        assert!(res.title.contains("Golden Axe HD 1.0.0"), "{}", res.title);
        assert!(
            !res.can_revert,
            "la primera instalación no tiene a dónde volver"
        );

        // La segunda sí, y revertir funciona desde el mismo resultado.
        let r2 = crate::pack_install::install_local(&packs, &id, &path);
        let res2 = outcome_of(&id, "Golden Axe HD", "1.0.0", &r2);
        assert!(res2.can_revert);
        let rev = crate::pack_install::rollback(&packs, &res2.pack_id);
        assert!(revert_outcome_of(&res2.pack_id, &rev).can_revert);
    }
}
