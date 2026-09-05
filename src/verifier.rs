//! verifier.rs — auditoría de un `.ay` que llegó por el disco.
//!
//! # Lo que CE puede afirmar de un pack, y lo que no
//!
//! Son dos preguntas distintas y sólo una tiene respuesta sin un tercero:
//!
//! * **¿Está completo y sin corromper?** Se contesta con el propio archivo:
//!   `integrity.toml` lista cada entrada con su `sha256` y su tamaño, y se
//!   comprueban todas. Es una auditoría offline, no necesita a nadie, y **es la
//!   que CE hace**.
//! * **¿Es de quien dice ser?** No se contesta con el archivo. Una firma se
//!   juzga contra una clave, y saber que *esa* clave es la del autor que dice
//!   ser necesita un registro de confianza — un tercero. **CE no lo tiene, y
//!   por eso no lo afirma.**
//!
//! Esa es la diferencia con AYTHER Play, que consulta el registro del Hub
//! (ADR-005). Acá no hay un «no pude verificar la firma» que se pueda leer como
//! un problema del pack: directamente **no hay una variante del tipo que diga
//! que un pack es auténtico**, porque este binario no puede saberlo. Un
//! indicador de confianza que en realidad sólo dice «el archivo no está
//! corrupto» es peor que ninguno: entrena a confiar.
//!
//! # Por qué CE audita y la edición paga puede saltearse
//!
//! En Play, un pack que viene del Hub llega con el sello del servicio, y ése es
//! el que decide; la auditoría por entrada es redundante y el motor la repite
//! al abrir. Acá no hay sello. **La auditoría es lo único que hay**, así que se
//! hace siempre y antes de decir «instalado».
//!
//! El esquema tiene que coincidir con `ay_pack` y con `core/src/archive_vfs.rs`.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// Lo que CE puede decir de un `.ay`.
///
/// **No hay una variante «auténtico».** Ver el comentario del módulo: este
/// binario no tiene con qué afirmarlo, y un tipo que ofrezca decirlo invita a
/// que alguien lo devuelva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audit {
    /// Cada entrada cierra con lo que `integrity.toml` declara: tamaño y hash.
    /// El pack está completo y nadie lo tocó desde que se horneó.
    Complete,
    /// No trae `integrity.toml`, así que no hay contra qué auditarlo. No es
    /// «malo»: es un pack viejo, de antes de #332. Se puede usar, y el motor lo
    /// vuelve a mirar al abrirlo — pero CE no puede afirmar que esté completo.
    MissingIndex,
    /// Algo no cierra, y se dice QUÉ: un pack corrupto y uno adulterado se ven
    /// igual desde acá, y el jugador merece el detalle para decidir si vale la
    /// pena volver a bajarlo.
    Mismatch(String),
    /// Ni siquiera es un `.ay`.
    InvalidPack(String),
}

impl Audit {
    /// ¿Se puede instalar? Un pack sin índice se instala igual —es la mayoría
    /// de los packs viejos— y uno que no cierra, no.
    pub fn is_installable(&self) -> bool {
        matches!(self, Self::Complete | Self::MissingIndex)
    }

    /// Lo que la ficha muestra. En palabras y no en un ícono: el estado de un
    /// pack es de las cosas que no pueden depender sólo del color (#629).
    ///
    /// La frase vive ACÁ, con el tipo que la decide, y pasa por el catálogo
    /// acá mismo (#585). Devolverla cruda y traducirla en cada pantalla es
    /// cómo dos superficies terminan diciendo cosas distintas del mismo pack.
    pub fn word(&self) -> String {
        match self {
            Self::Complete => tr!("Completo y sin alterar"),
            Self::MissingIndex => tr!("No trae índice: no se puede auditar"),
            Self::Mismatch(_) => tr!("El archivo no cierra"),
            Self::InvalidPack(_) => tr!("No es un pack .ay"),
        }
    }
}

#[derive(serde::Deserialize)]
struct IntegrityToml {
    #[serde(default)]
    entry: Vec<IntegrityEntry>,
}

#[derive(serde::Deserialize)]
struct IntegrityEntry {
    path: String,
    sha256: String,
    size: u64,
}

/// Audita el `.ay` de `path`. No lo abre para jugar: lee el ZIP y compara.
pub fn audit(path: &Path) -> Audit {
    match audit_inner(path) {
        Ok(a) => a,
        Err(e) => Audit::InvalidPack(e),
    }
}

fn audit_inner(path: &Path) -> Result<Audit, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("no se pudo abrir: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("no es un ZIP: {e}"))?;

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("entrada {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut data)
            .map_err(|e| format!("leyendo {name}: {e}"))?;
        files.insert(name, data);
    }

    let Some(index) = files.get("integrity.toml") else {
        return Ok(Audit::MissingIndex);
    };

    let parsed: IntegrityToml = toml::from_str(
        std::str::from_utf8(index).map_err(|_| "integrity.toml no es UTF-8".to_string())?,
    )
    .map_err(|e| format!("integrity.toml mal formado: {e}"))?;

    let listed_entries: BTreeMap<&str, &IntegrityEntry> =
        parsed.entry.iter().map(|e| (e.path.as_str(), e)).collect();

    // Las dos direcciones, y las dos importan. Que cada archivo presente esté
    // listado y cierre atrapa lo agregado y lo modificado; que cada entrada
    // listada esté presente atrapa lo que falta. Con una sola, un pack al que
    // le sacaron un asset pasaría por íntegro.
    let mut visited_entries = 0usize;
    for (name, data) in &files {
        if name == "integrity.toml" || name == "signature.bin" {
            continue;
        }
        let Some(e) = listed_entries.get(name.as_str()) else {
            return Ok(Audit::Mismatch(format!("«{name}» no está en el índice")));
        };
        if e.size != data.len() as u64 {
            return Ok(Audit::Mismatch(format!(
                "«{name}» mide {} y el índice dice {}",
                data.len(),
                e.size
            )));
        }
        let hash = format!("{:x}", Sha256::digest(data));
        if !hash.eq_ignore_ascii_case(&e.sha256) {
            return Ok(Audit::Mismatch(format!("«{name}» no coincide con su hash")));
        }
        visited_entries += 1;
    }
    if visited_entries != listed_entries.len() {
        return Ok(Audit::Mismatch(format!(
            "faltan {} archivo(s) que el índice declara",
            listed_entries.len() - visited_entries
        )));
    }

    Ok(Audit::Complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pack(entries: &[(&str, &[u8])], with_index: bool) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        let mut z = zip::ZipWriter::new(f.reopen().unwrap());
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut toml_txt = String::new();
        for (n, d) in entries {
            z.start_file(*n, options).unwrap();
            z.write_all(d).unwrap();
            toml_txt.push_str(&format!(
                "[[entry]]\npath = \"{n}\"\nsha256 = \"{:x}\"\nsize = {}\n",
                Sha256::digest(d),
                d.len()
            ));
        }
        if with_index {
            z.start_file("integrity.toml", options).unwrap();
            z.write_all(toml_txt.as_bytes()).unwrap();
        }
        z.finish().unwrap();
        f
    }

    #[test]
    fn a_complete_pack_checks_out() {
        let f = pack(&[("a.bin", b"hola"), ("b/c.bin", b"chau")], true);
        assert_eq!(audit(f.path()), Audit::Complete);
    }

    /// La dirección que se olvida: un pack al que le SACARON un archivo.
    ///
    /// Comprobar sólo que lo presente esté listado y cierre lo deja pasar —
    /// todo lo que quedó está bien—. Por eso se cuentan las dos puntas.
    #[test]
    fn a_pack_missing_something_does_not_check_out() {
        let f = pack(&[("a.bin", b"hola"), ("b.bin", b"chau")], true);
        // Se re-arma sin `b.bin` pero con el índice que lo declara.
        let g = tempfile::NamedTempFile::new().unwrap();
        {
            let orig = std::fs::File::open(f.path()).unwrap();
            let mut z = zip::ZipArchive::new(orig).unwrap();
            let mut w = zip::ZipWriter::new(g.reopen().unwrap());
            let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for i in 0..z.len() {
                let mut e = z.by_index(i).unwrap();
                let n = e.name().to_string();
                if n == "b.bin" {
                    continue;
                }
                let mut d = Vec::new();
                e.read_to_end(&mut d).unwrap();
                w.start_file(n, o).unwrap();
                w.write_all(&d).unwrap();
            }
            w.finish().unwrap();
        }
        match audit(g.path()) {
            Audit::Mismatch(m) => assert!(m.contains("falta"), "y dice qué pasó: {m}"),
            other_result => {
                panic!("un pack incompleto tiene que salir señalado, salió {other_result:?}")
            }
        }
    }

    #[test]
    fn a_pack_without_index_cannot_be_audited_but_can_be_used() {
        let f = pack(&[("a.bin", b"hola")], false);
        let a = audit(f.path());
        assert_eq!(a, Audit::MissingIndex);
        assert!(a.is_installable(), "los packs viejos siguen andando");
    }

    #[test]
    fn what_is_not_a_pack_is_said_as_such() {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), b"no soy un zip").unwrap();
        assert!(matches!(audit(f.path()), Audit::InvalidPack(_)));
    }

    /// EL ORÁCULO DE LO QUE CE NO PUEDE DECIR.
    ///
    /// No hay ninguna variante que afirme autenticidad, y no puede haberla: CE
    /// no tiene un registro de claves contra el cual juzgar una firma. Si
    /// alguien agrega una, este test se cae — y esa caída es la conversación
    /// que hay que tener antes de mostrarle un candado al jugador.
    #[test]
    fn ce_does_not_claim_who_a_pack_belongs_to() {
        for a in [
            Audit::Complete,
            Audit::MissingIndex,
            Audit::Mismatch(String::new()),
            Audit::InvalidPack(String::new()),
        ] {
            let p = a.word().to_lowercase();
            assert!(
                !p.contains("firm")
                    && !p.contains("autén")
                    && !p.contains("verificad")
                    && !p.contains("confia"),
                "«{}» sugiere que CE juzgó la PROCEDENCIA del pack, y no puede: sin un \
                 registro de claves, una firma válida sólo dice que alguien firmó — no quién. \
                 Ver el comentario del módulo (ADR-005).",
                a.word()
            );
        }
    }
}
