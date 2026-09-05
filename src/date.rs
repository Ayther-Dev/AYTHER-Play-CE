//! date.rs — la hora UTC, escrita como la lee una persona (#590).
//!
//! El registro de aceptación de los términos lleva **fecha**, y una fecha que
//! sólo se puede leer con una calculadora no sirve para lo que existe: que
//! alguien pueda abrir `play_config.toml` y ver cuándo se aceptó qué. Un
//! `1787869243` no responde esa pregunta.
//!
//! Se escribe a mano y no con `chrono` porque el workspace no lo tiene y traer
//! una dependencia entera —con su árbol— para formatear catorce números es caro
//! para lo que da. El algoritmo es el de Howard Hinnant (`civil_from_days`), que
//! es aritmética entera pura: sin tablas, sin bucle por año y sin casos
//! especiales para los bisiestos.
//!
//! Todo lo que decide vive en `de_epoch`, que es una función de un entero a un
//! `String`. `ahora_utc()` es la única parte que mira el reloj, y por eso es la
//! única que no se puede probar.

use std::time::{SystemTime, UNIX_EPOCH};

/// Ahora, en UTC, como `2026-08-26T22:14:03Z`.
///
/// Un reloj que se fue antes de 1970 —una máquina con la BIOS en cero, que
/// existe— devuelve el epoch en vez de fallar: la aceptación es lo que importa
/// y perderla por una fecha rara sería peor que anotar una fecha rara.
/// #594: ahora, en segundos desde epoch.
///
/// Es el formato que se guarda en la biblioteca —y no el ISO-8601 de
/// `ahora_utc`— porque lo que se hace con esta fecha es COMPARARLA: el orden
/// «recientes» ordena por ella y «Continuar» busca la mayor. Un entero se
/// compara con `>`; una cadena habría que parsearla en cada comparación, o
/// confiar en que el orden lexicográfico coincida con el cronológico, que es
/// cierto en ISO-8601 hasta que alguien escribe una zona horaria.
pub fn epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn now_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    from_epoch(secs)
}

/// #603 — el ahora en compacto: `20260827T211500Z`.
///
/// Es el mismo instante que `ahora_utc()`, sin los separadores. Va en el NOMBRE
/// de cada versión guardada, y un nombre de archivo no puede llevar `:`. Ordena
/// alfabéticamente igual que cronológicamente, que es lo que deja listar las
/// versiones sin parsear ninguna.
pub fn compact_timestamp() -> String {
    now_utc()
        .chars()
        .filter(|c| *c != '-' && *c != ':')
        .collect()
}

/// #604 — una marca compacta, de vuelta a algo que una persona lee.
///
/// `20260827T211500Z` → `2026-08-27 21:15`. Sin los segundos: a la pregunta
/// «¿de cuándo es mi partida?» le sobran, y a distancia de TV cada carácter de
/// más es uno menos que se lee de un vistazo.
///
/// Lo que no entienda se devuelve tal cual: una marca rara es mejor mostrada
/// cruda que reemplazada por una fecha inventada.
pub fn format_compact_timestamp(timestamp: &str) -> String {
    let b = timestamp.as_bytes();
    if b.len() < 15 || b[8] != b'T' {
        return timestamp.to_string();
    }
    let t = |i: usize, j: usize| std::str::from_utf8(&b[i..j]).unwrap_or("");
    format!(
        "{}-{}-{} {}:{}",
        t(0, 4),
        t(4, 6),
        t(6, 8),
        t(9, 11),
        t(11, 13)
    )
}

/// Segundos desde el epoch → ISO 8601 UTC.
pub fn from_epoch(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let remaining = secs.rem_euclid(86_400);
    let (a, m, d) = civil_from_days(days);
    let (h, mi, s) = (remaining / 3600, (remaining % 3600) / 60, remaining % 60);
    format!("{a:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Días desde 1970-01-01 → (año, mes, día). Hinnant, `civil_from_days`.
///
/// La era de 400 años es el truco: el calendario gregoriano se repite exacto
/// cada 400 años (146.097 días), así que dividir por ahí saca los bisiestos del
/// camino de una vez y lo que queda es aritmética sin excepciones.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468; // origen movido a 0000-03-01
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // día dentro de la era: 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // 0..=399
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // 0..=365, desde el 1 de marzo
    let mp = (5 * doy + 2) / 153; // 0..=11, con marzo = 0
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_the_epoch() {
        assert_eq!(from_epoch(0), "1970-01-01T00:00:00Z");
    }

    /// Fechas conocidas, incluidos los dos casos donde el calendario tiene
    /// trampa: el 29 de febrero de un bisiesto y el 1 de marzo del año 2000,
    /// que es bisiesto aunque sea múltiplo de 100.
    #[test]
    fn known_dates_match_what_the_almanacs_say() {
        for (secs, expected) in [
            (1_000_000_000_i64, "2001-09-09T01:46:40Z"),
            (951_782_400, "2000-02-29T00:00:00Z"), // bisiesto de siglo
            (951_868_800, "2000-03-01T00:00:00Z"),
            (1_078_012_800, "2004-02-29T00:00:00Z"),
            (1_788_000_000, "2026-08-29T10:40:00Z"),
            (2_147_483_647, "2038-01-19T03:14:07Z"), // el día que se rompe i32
        ] {
            assert_eq!(from_epoch(secs), expected, "epoch {secs}");
        }
    }

    /// Un reloj anterior a 1970 no puede escribir una fecha con un año
    /// negativo pegado a un mes válido: el registro quedaría ilegible y, peor,
    /// se ordenaría mal contra los demás. La división euclídea es lo que hace
    /// que el día anterior al epoch sea el 31 de diciembre y no el «día -1».
    #[test]
    fn the_day_before_the_epoch_is_new_years_eve() {
        assert_eq!(from_epoch(-1), "1969-12-31T23:59:59Z");
        assert_eq!(from_epoch(-86_400), "1969-12-31T00:00:00Z");
    }

    /// La forma es fija: 20 caracteres, siempre. Un ancho variable rompe
    /// cualquier comparación de texto entre dos registros.
    #[test]
    fn the_width_never_changes() {
        for s in [0, 1, 1_788_000_000, 253_402_300_799] {
            let t = from_epoch(s);
            assert_eq!(t.len(), 20, "«{t}»");
            assert!(t.ends_with('Z'));
        }
    }

    /// Y lo que escribe el reloj tiene la misma forma. Es lo único que se
    /// puede afirmar sin congelar el tiempo, pero es lo que hace falta: que
    /// `ahora_utc` no devuelva otra cosa que una fecha.
    #[test]
    fn the_clock_writes_a_real_date() {
        let t = now_utc();
        assert_eq!(t.len(), 20, "«{t}»");
        assert!(t.starts_with("20"), "«{t}»: ¿qué año es?");
        assert_eq!(&t[4..5], "-");
        assert_eq!(&t[10..11], "T");
    }
}
