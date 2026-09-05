#![allow(dead_code)]
//! Lo que las fuentes vendorizadas de Play saben dibujar.
//!
//! Un lector mínimo de la tabla `cmap` de un `.ttf`, compartido por los dos
//! oráculos de tipografía: `glyphs.rs` (ningún carácter fuera de las fuentes) e
//! `icons.rs` (ningún icono fuera de Phosphor). Vive acá y no duplicado en
//! cada uno porque un parser copiado se arregla en una sola de las copias.
//!
//! Es mínimo a propósito: traer una caja de tipografía entera para leer una
//! tabla de rangos sería una dependencia de producción para un test.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn u16be(b: &[u8], o: usize) -> u32 {
    ((b[o] as u32) << 8) | b[o + 1] as u32
}
fn u32be(b: &[u8], o: usize) -> u32 {
    ((b[o] as u32) << 24) | ((b[o + 1] as u32) << 16) | ((b[o + 2] as u32) << 8) | b[o + 3] as u32
}

/// Los codepoints que este `.ttf` mapea.
pub fn cmap_of(path: &Path) -> HashSet<u32> {
    let mut out = HashSet::new();
    let Ok(b) = std::fs::read(path) else {
        return out;
    };
    if b.len() < 12 {
        return out;
    }

    // Directorio de tablas → offset de `cmap`.
    let table_count = u16be(&b, 4) as usize;
    let mut cmap = None;
    for i in 0..table_count {
        let r = 12 + i * 16;
        if r + 16 > b.len() {
            break;
        }
        if &b[r..r + 4] == b"cmap" {
            cmap = Some(u32be(&b, r + 8) as usize);
        }
    }
    let Some(cmap) = cmap else { return out };
    if cmap + 4 > b.len() {
        return out;
    }

    // Se recorren TODAS las subtablas Unicode y se unen. Quedarse con la
    // «mejor» obligaría a elegir, y unir no puede equivocarse: un codepoint que
    // cualquier subtabla mapea es un codepoint que la fuente dibuja.
    let n_sub = u16be(&b, cmap + 2) as usize;
    for i in 0..n_sub {
        let r = cmap + 4 + i * 8;
        if r + 8 > b.len() {
            break;
        }
        let (plat, enc) = (u16be(&b, r), u16be(&b, r + 2));
        let unicode = plat == 0 || (plat == 3 && (enc == 1 || enc == 10));
        if !unicode {
            continue;
        }
        let sub = cmap + u32be(&b, r + 4) as usize;
        if sub + 4 > b.len() {
            continue;
        }
        match u16be(&b, sub) {
            4 => {
                let seg2 = u16be(&b, sub + 6) as usize;
                let seg = seg2 / 2;
                let (end, start) = (sub + 14, sub + 16 + seg2);
                for s in 0..seg {
                    if start + s * 2 + 2 > b.len() {
                        break;
                    }
                    let (e, i0) = (u16be(&b, end + s * 2), u16be(&b, start + s * 2));
                    if i0 > e || e == 0xFFFF && i0 == 0xFFFF {
                        continue;
                    }
                    // No se resuelve el glyph id: alcanza con que el rango
                    // exista. Un rango declarado que apunte al glifo 0 sería un
                    // archivo roto, no un caso que este test tenga que cubrir.
                    for c in i0..=e {
                        out.insert(c);
                    }
                }
            }
            12 => {
                let n = u32be(&b, sub + 12) as usize;
                for g in 0..n {
                    let r = sub + 16 + g * 12;
                    if r + 12 > b.len() {
                        break;
                    }
                    let (i0, e) = (u32be(&b, r), u32be(&b, r + 4));
                    if i0 > e || e - i0 > 0x10000 {
                        continue;
                    }
                    for c in i0..=e {
                        out.insert(c);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Todo lo que las fuentes vendorizadas saben dibujar, junto.
pub fn coverage() -> HashSet<u32> {
    let dir = root().join("fonts");
    let mut out = HashSet::new();
    let mut n = 0;
    for e in std::fs::read_dir(&dir)
        .expect("no hay play/fonts/")
        .flatten()
    {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("ttf") {
            out.extend(cmap_of(&p));
            n += 1;
        }
    }
    assert!(
        n >= 7,
        "se esperaban las 7 fuentes vendorizadas (6 de texto + Phosphor), se leyeron {n}"
    );
    assert!(
        out.len() > 200,
        "la cmap salió sospechosamente chica ({}): parser roto",
        out.len()
    );
    out
}
