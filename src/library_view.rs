//! library_view.rs — qué juegos se ven y en qué orden (#593).
//!
//! Vive aparte de `library.rs` porque son dos trabajos distintos: aquél LEE el
//! disco y mantiene la caché; éste decide qué mostrar de lo que ya está en
//! memoria. Separarlos es lo que deja probar el filtrado sin tocar un archivo.
//!
//! # Lo automático y lo del jugador no se pisan
//!
//! El nombre y la región los pone el DAT en cada escaneo; la carátula, el
//! barrido de `covers/`. Si una edición del jugador escribiera ENCIMA de esos
//! campos, el próximo escaneo la borraría —o peor, la conservaría y no habría
//! forma de volver a lo automático—. Por eso lo que el jugador escribe vive en
//! campos PROPIOS (`titulo_usuario`, `arte_usuario`) y lo automático queda
//! intacto debajo: «restaurar valores automáticos» es vaciar el override, no
//! re-derivar nada, y no puede fallar.

use crate::library::{Game, PackStatus};

/// Cómo se ordena la biblioteca.
///
/// `Recientes` va primero en la lista de la UI pero no es el default: con la
/// biblioteca recién escaneada nadie jugó nada todavía, y un orden donde todo
/// empata se ve como aleatorio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Alphabetical,
    ReverseAlphabetical,
    Platform,
    Recent,
}

impl Order {
    pub const ALL: [Order; 4] = [
        Order::Alphabetical,
        Order::ReverseAlphabetical,
        Order::Platform,
        Order::Recent,
    ];
    pub fn from_index(i: i32) -> Order {
        *Self::ALL
            .get(i.max(0) as usize)
            .unwrap_or(&Order::Alphabetical)
    }
}

/// Qué se muestra. **No se persiste a disco**: el CA de #593 dice «los filtros
/// persisten por SESIÓN». Un filtro guardado es una trampa — se abre Play al día
/// siguiente, faltan tres cuartos de la biblioteca y no hay nada en pantalla que
/// explique por qué. Los favoritos y lo oculto sí se guardan, porque son
/// decisiones sobre el juego y no sobre la vista de hoy.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub text: String,
    /// "" = todas.
    pub platform: String,
    /// Sólo los que tienen pack y está sano.
    pub compatible_only: bool,
    pub favorites_only: bool,
    /// Los ocultos no se ven salvo que se los pida. No se BORRAN: ocultar es
    /// una decisión reversible y borrar una entrada perdería su favorito, su
    /// título editado y su tiempo de juego.
    pub show_hidden: bool,
}

impl Filter {
    /// ¿Hay algo filtrando? La UI lo necesita para poder decir «0 de 120 juegos»
    /// en vez de «no hay juegos», que es lo que un filtro olvidado parece.
    pub fn active(&self) -> bool {
        !self.text.trim().is_empty()
            || !self.platform.is_empty()
            || self.compatible_only
            || self.favorites_only
    }
}

/// Normaliza para buscar: minúsculas y sin tildes.
///
/// Sin esto, buscar «pokemon» no encuentra «Pokémon» y buscar «Sonic» no
/// encuentra «SONIC» — y el jugador concluye que el juego no está. Se resuelve a
/// mano y no con una dependencia porque el alfabeto que importa son las cinco
/// vocales acentuadas del español y la eñe; traer una caja de Unicode entera
/// para eso sería pagar de más.
pub fn normalize_search_text(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other_filter => other_filter,
        })
        .collect()
}

/// ¿Este juego pasa el filtro?
fn matches_filter(g: &Game, f: &Filter, query: &str) -> bool {
    if g.hidden && !f.show_hidden {
        return false;
    }
    if f.favorites_only && !g.favorite {
        return false;
    }
    if !f.platform.is_empty() && g.platform != f.platform {
        return false;
    }
    // #595: «compatible» es el GRADO del validador, no «tiene pack y se lee».
    // Un pack con advertencias sigue siendo compatible —anda, le falta algo—;
    // uno no verificado no, porque nadie dijo que sí.
    if f.compatible_only && !matches!(g.pack_status, PackStatus::Exact | PackStatus::Warnings) {
        return false;
    }
    if !query.is_empty() {
        // Se busca sobre el título VISIBLE. Buscar sobre el automático haría que
        // un juego renombrado por el jugador no aparezca al escribir el nombre
        // que él mismo le puso, que es el único que recuerda.
        if !normalize_search_text(&g.title()).contains(query) {
            return false;
        }
    }
    true
}

/// Los ÍNDICES de `games` que se ven, ya ordenados.
///
/// Devuelve índices y no juegos clonados: la lista se recalcula en cada tecla de
/// la búsqueda, y copiar la biblioteca entera por pulsación es trabajo que se
/// nota en una máquina modesta —que es donde este launcher tiene que andar—.
pub fn visible_indices(games: &[Game], f: &Filter, order: Order) -> Vec<usize> {
    let query = normalize_search_text(f.text.trim());
    let mut idx: Vec<usize> = games
        .iter()
        .enumerate()
        .filter(|(_, g)| matches_filter(g, f, &query))
        .map(|(i, _)| i)
        .collect();

    // Los favoritos van SIEMPRE arriba, dentro de cualquier orden. Es lo que
    // hace que marcar un favorito sirva de algo sin tener que además filtrar.
    idx.sort_by(|a, b| {
        let (ga, gb) = (&games[*a], &games[*b]);
        gb.favorite
            .cmp(&ga.favorite)
            .then_with(|| match order {
                Order::Alphabetical => {
                    normalize_search_text(&ga.title()).cmp(&normalize_search_text(&gb.title()))
                }
                Order::ReverseAlphabetical => {
                    normalize_search_text(&gb.title()).cmp(&normalize_search_text(&ga.title()))
                }
                Order::Platform => ga.platform.cmp(&gb.platform).then_with(|| {
                    normalize_search_text(&ga.title()).cmp(&normalize_search_text(&gb.title()))
                }),
                // Más reciente primero; los que nunca se jugaron caen al final por
                // el 0, y entre ellos se ordenan alfabéticamente en vez de quedar
                // en el orden en que el sistema de archivos los devolvió.
                Order::Recent => gb.last_played.cmp(&ga.last_played).then_with(|| {
                    normalize_search_text(&ga.title()).cmp(&normalize_search_text(&gb.title()))
                }),
            })
            .then_with(|| ga.id.cmp(&gb.id))
            .then_with(|| ga.path.cmp(&gb.path))
    });
    idx
}

/// Las plataformas presentes, para armar los chips. Sale de lo que HAY y no de
/// la tabla de extensiones: un chip de una consola de la que el jugador no tiene
/// ni un juego es ruido que además nunca se puede sacar.
pub fn platforms(games: &[Game]) -> Vec<String> {
    let mut v: Vec<String> = games
        .iter()
        .filter(|g| !g.hidden)
        .map(|g| g.platform.clone())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// #594 — el juego que «Continuar» ofrece: el último que se jugó.
///
/// `None` cuando nunca se jugó nada, y eso NO es un caso raro que la UI pueda
/// resolver con un valor por defecto: una biblioteca recién escaneada no tiene
/// ninguno, y ofrecer «Continuar» con el primero alfabético sería inventar una
/// partida que no existió. Con `None`, Inicio muestra otra cosa.
///
/// Los OCULTOS no cuentan: sacar un juego de la vista y encontrarlo igual en el
/// hero de Inicio sería no haberlo ocultado.
pub fn continue_game(games: &[Game]) -> Option<usize> {
    games
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.hidden && g.last_played > 0)
        .max_by_key(|(_, g)| g.last_played)
        .map(|(i, _)| i)
}

/// #594 — los últimos `n` jugados, del más reciente al más viejo, SIN el que ya
/// está en «Continuar»: repetirlo al lado suyo gasta la fila más visible de la
/// pantalla en decir dos veces lo mismo.
pub fn recent(games: &[Game], n: usize) -> Vec<usize> {
    let hero = continue_game(games);
    let mut v: Vec<usize> = games
        .iter()
        .enumerate()
        .filter(|(i, g)| !g.hidden && g.last_played > 0 && Some(*i) != hero)
        .map(|(i, _)| i)
        .collect();
    v.sort_by_key(|i| std::cmp::Reverse(games[*i].last_played));
    v.truncate(n);
    v
}

/// #594 — «12 h 40 m», «40 m», «recién empezado».
///
/// No se muestran segundos ni por debajo del minuto: un contador que dice
/// «43 s» invita a mirarlo, y el dato acá sirve para reconocer un juego —«a éste
/// le metí horas»—, no para cronometrar. Menos de un minuto tampoco es «0 m»,
/// que se lee como que no quedó registrado.
pub fn format_play_time(seconds: u64) -> String {
    if seconds < 60 {
        return "recién empezado".to_string();
    }
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    if h == 0 {
        format!("{m} m")
    } else {
        format!("{h} h {m} m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str, platform: &str) -> Game {
        Game {
            id: String::new(),
            sha256: String::new(),
            revision: None,
            name: name.into(),
            path: format!("D:/roms/{name}.md"),
            platform: platform.into(),
            crc32: "00000000".into(),
            region: "WLD".into(),
            has_pack: false,
            pack_path: String::new(),
            available_packs: Vec::new(),
            pack_status: PackStatus::NoPack,
            compat_reason: String::new(),
            compat_codes: Vec::new(),
            cover_path: String::new(),
            size_bytes: 1,
            mtime: 1,
            favorite: false,
            hidden: false,
            user_title: None,
            user_art: None,
            last_played: 0,
            seconds_played: 0,
        }
    }

    fn names(games: &[Game], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|i| games[*i].title()).collect()
    }

    /// EL CA de #593: editar el título NO destruye lo automático.
    ///
    /// El nombre del DAT queda intacto debajo del override, así que «restaurar
    /// valores automáticos» es vaciar un campo y no re-derivar nada — no puede
    /// fallar, y no depende de que el DAT siga estando.
    #[test]
    fn editing_the_title_does_not_erase_the_automatic_one() {
        let mut g = game("Golden Axe (World) (Rev A)", "Mega Drive");
        assert_eq!(g.title(), "Golden Axe (World) (Rev A)");

        g.user_title = Some("Golden Axe".into());
        assert_eq!(g.title(), "Golden Axe", "manda lo que escribió el jugador");
        assert_eq!(
            g.name, "Golden Axe (World) (Rev A)",
            "lo automatico sigue ahi"
        );

        g.user_title = None; // «restaurar valores automáticos»
        assert_eq!(g.title(), "Golden Axe (World) (Rev A)");
    }

    /// Un título vacío o en blanco NO es un override: es alguien que borró el
    /// campo. Guardarlo dejaría una tarjeta sin nombre y sin forma de volver.
    #[test]
    fn a_blank_title_falls_back_to_the_automatic_one() {
        let mut g = game("Sonic", "Mega Drive");
        g.user_title = Some("   ".into());
        assert_eq!(g.title(), "Sonic");
    }

    /// La búsqueda no distingue mayúsculas ni tildes: quien escribe «pokemon»
    /// tiene que encontrar «Pokémon», y si no lo encuentra concluye que el juego
    /// no está en su biblioteca.
    #[test]
    fn search_ignores_case_and_accents() {
        let games = vec![game("Pokémon Rojo", "GBA"), game("SONIC", "Mega Drive")];
        let mut f = Filter {
            text: "pokemon".into(),
            ..Default::default()
        };
        assert_eq!(
            names(&games, &visible_indices(&games, &f, Order::Alphabetical)),
            ["Pokémon Rojo"]
        );

        f.text = "sonic".into();
        assert_eq!(
            names(&games, &visible_indices(&games, &f, Order::Alphabetical)),
            ["SONIC"]
        );

        // Y busca sobre el titulo VISIBLE: un juego renombrado se encuentra por
        // el nombre que el jugador le puso, que es el unico que recuerda.
        let mut games2 = games.clone();
        games2[0].user_title = Some("Rojo".into());
        f.text = "rojo".into();
        assert_eq!(
            names(&games2, &visible_indices(&games2, &f, Order::Alphabetical)),
            ["Rojo"]
        );
    }

    /// Los favoritos van arriba de cualquier orden. Sin eso, marcar un favorito
    /// no hace nada visible salvo que además se filtre por favoritos, y una
    /// marca que no se nota no se usa.
    #[test]
    fn favorites_top_any_ordering() {
        let mut games = vec![game("Alfa", "MD"), game("Beta", "MD"), game("Zeta", "MD")];
        games[2].favorite = true;

        let f = Filter::default();
        assert_eq!(
            names(&games, &visible_indices(&games, &f, Order::Alphabetical)),
            ["Zeta", "Alfa", "Beta"]
        );
        assert_eq!(
            names(
                &games,
                &visible_indices(&games, &f, Order::ReverseAlphabetical)
            ),
            ["Zeta", "Beta", "Alfa"]
        );
    }

    /// Ocultar saca de la vista sin borrar: el favorito, el título editado y el
    /// tiempo jugado siguen ahí, y volver a mostrarlos los devuelve enteros.
    #[test]
    fn hiding_is_not_deleting() {
        let mut games = vec![game("Visible", "MD"), game("Escondido", "MD")];
        games[1].hidden = true;
        games[1].favorite = true;
        games[1].user_title = Some("Mi nombre".into());

        let mut f = Filter::default();
        assert_eq!(
            names(&games, &visible_indices(&games, &f, Order::Alphabetical)),
            ["Visible"]
        );
        assert_eq!(platforms(&games), ["MD"], "un oculto no aporta su chip");

        f.show_hidden = true;
        let v = visible_indices(&games, &f, Order::Alphabetical);
        assert_eq!(v.len(), 2);
        assert_eq!(
            games[v[0]].title(),
            "Mi nombre",
            "el favorito escondido, entero"
        );
    }

    /// Un oculto sigue ocultándose aunque esté marcado como favorito: ocultar es
    /// la decisión más específica de las dos.
    #[test]
    fn hidden_beats_favorite() {
        let mut games = vec![game("Uno", "MD")];
        games[0].hidden = true;
        games[0].favorite = true;
        let f = Filter {
            favorites_only: true,
            ..Default::default()
        };
        assert!(visible_indices(&games, &f, Order::Alphabetical).is_empty());
    }

    /// «Compatibles» es tener pack Y que esté sano. Un pack corrupto no es
    /// compatible, y sin pack no hay nada que ser.
    #[test]
    fn compatible_requires_a_healthy_pack() {
        let mut games = vec![
            game("Sin pack", "MD"),
            game("Con pack", "MD"),
            game("Roto", "MD"),
        ];
        games[1].has_pack = true;
        games[1].pack_status = PackStatus::Exact;
        games[2].has_pack = true;
        games[2].pack_status = PackStatus::Incompatible;

        let f = Filter {
            compatible_only: true,
            ..Default::default()
        };
        assert_eq!(
            names(&games, &visible_indices(&games, &f, Order::Alphabetical)),
            ["Con pack"]
        );
    }

    /// El orden por recientes manda a los que nunca se jugaron al final, y entre
    /// ellos los deja alfabéticos: sin el desempate quedarían en el orden en que
    /// el sistema de archivos los devolvió, que se ve como aleatorio.
    #[test]
    fn recents_break_ties_alphabetically() {
        let mut games = vec![game("Zeta", "MD"), game("Alfa", "MD"), game("Jugado", "MD")];
        games[2].last_played = 1000;

        let f = Filter::default();
        assert_eq!(
            names(&games, &visible_indices(&games, &f, Order::Recent)),
            ["Jugado", "Alfa", "Zeta"]
        );
    }

    // -----------------------------------------------------------------------
    // #594 — Continuar, recientes y tiempo jugado
    // -----------------------------------------------------------------------

    /// Sin nada jugado no hay «Continuar», y eso tiene que poder decirse.
    /// Ofrecer el primero alfabético sería inventar una partida que no existió,
    /// y el jugador apretaría A esperando volver a donde estaba.
    #[test]
    fn with_no_plays_there_is_no_continue() {
        let games = vec![game("Alfa", "MD"), game("Beta", "MD")];
        assert_eq!(continue_game(&games), None);
        assert!(recent(&games, 6).is_empty());
    }

    /// «Continuar» es el ÚLTIMO jugado, y no aparece además en recientes:
    /// repetirlo al lado suyo gasta la fila más visible en decir dos veces lo
    /// mismo.
    #[test]
    fn continue_is_the_latest_and_does_not_repeat_in_recents() {
        let mut games = vec![
            game("Viejo", "MD"),
            game("Nuevo", "MD"),
            game("Medio", "MD"),
            game("Nunca", "MD"),
        ];
        games[0].last_played = 100;
        games[1].last_played = 300;
        games[2].last_played = 200;

        assert_eq!(
            continue_game(&games).map(|i| games[i].title()),
            Some("Nuevo".into())
        );
        assert_eq!(
            names(&games, &recent(&games, 6)),
            ["Medio", "Viejo"],
            "del mas reciente al mas viejo, y sin el hero ni el no-jugado"
        );
    }

    /// Un juego oculto no vuelve por la puerta de Inicio: sacarlo de la vista y
    /// encontrarlo igual en el hero sería no haberlo ocultado.
    #[test]
    fn hidden_games_do_not_come_back_through_home() {
        let mut games = vec![game("Escondido", "MD"), game("Visible", "MD")];
        games[0].last_played = 300;
        games[0].hidden = true;
        games[1].last_played = 100;

        assert_eq!(
            continue_game(&games).map(|i| games[i].title()),
            Some("Visible".into())
        );
        assert!(
            recent(&games, 6).is_empty(),
            "el hero se llevo al unico visible"
        );
    }

    /// El tope de la fila se respeta: es una fila, no una biblioteca.
    #[test]
    fn recents_cuts_at_the_cap() {
        let mut games: Vec<Game> = (0..10).map(|i| game(&format!("J{i}"), "MD")).collect();
        for (i, g) in games.iter_mut().enumerate() {
            g.last_played = 100 + i as i64;
        }
        assert_eq!(recent(&games, 4).len(), 4);
        assert_eq!(
            games[recent(&games, 4)[0]].title(),
            "J8",
            "el mas reciente despues del hero (J9)"
        );
    }

    /// El tiempo se lee, no se cronometra: nada de segundos, y menos de un
    /// minuto no es «0 m» —que se lee como que no quedó registrado— sino que se
    /// dice con palabras.
    #[test]
    fn play_time_reads_at_a_glance() {
        assert_eq!(format_play_time(0), "recién empezado");
        assert_eq!(format_play_time(59), "recién empezado");
        assert_eq!(format_play_time(60), "1 m");
        assert_eq!(format_play_time(3599), "59 m");
        assert_eq!(format_play_time(3600), "1 h 0 m");
        assert_eq!(format_play_time(45_600), "12 h 40 m");
    }

    /// Los chips salen de lo que HAY. Un chip de una consola sin juegos es ruido
    /// que además nunca se puede sacar de la barra.
    #[test]
    fn the_chips_come_from_the_platforms_present() {
        let games = vec![
            game("A", "Mega Drive"),
            game("B", "SNES"),
            game("C", "Mega Drive"),
        ];
        assert_eq!(platforms(&games), ["Mega Drive", "SNES"]);
    }

    /// `activo()` es lo que deja a la UI decir «0 de 120» en vez de «no hay
    /// juegos»: un filtro olvidado y una biblioteca vacía se ven igual, y sólo
    /// uno de los dos tiene arreglo a un clic.
    #[test]
    fn the_filter_knows_whether_it_is_filtering() {
        let mut f = Filter::default();
        assert!(!f.active());
        f.text = "   ".into();
        assert!(!f.active(), "espacios en blanco no son una busqueda");
        f.text = "so".into();
        assert!(f.active());
    }
}
