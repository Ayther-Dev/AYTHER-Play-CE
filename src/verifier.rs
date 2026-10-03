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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[cfg(test)]
pub fn audit(path: &Path) -> Audit {
    match audit_with_progress(
        path,
        &std::sync::atomic::AtomicBool::new(false),
        &mut |_, _| {},
    ) {
        Ok(a) => a,
        Err(e) => Audit::InvalidPack(e),
    }
}

pub fn audit_with_progress(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Audit, String> {
    use ayther_core::pack_security::{MAX_ENTRY_BYTES, MAX_ENTRY_COUNT, MAX_METADATA_BYTES};
    use std::sync::atomic::Ordering;
    crate::cache_metrics::PACK_AUDIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    crate::cache_metrics::PACK_CONTENT_OPENS.fetch_add(1, Ordering::Relaxed);
    let file = std::fs::File::open(path).map_err(|e| format!("no se pudo abrir: {e}"))?;
    let mut zip = match zip::ZipArchive::new(file) {
        Ok(zip) => zip,
        Err(zip::result::ZipError::Io(error)) => {
            return Err(format!("No se pudo leer el pack: {error}"));
        }
        Err(error) => return Ok(Audit::InvalidPack(format!("no es un ZIP: {error}"))),
    };
    if zip.len() > MAX_ENTRY_COUNT {
        return Err("El pack excede el límite de entradas".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Comprobación cancelada".into());
    }
    let mut index = String::new();
    match zip.by_name("integrity.toml") {
        Ok(mut entry) => {
            if entry.size() > MAX_METADATA_BYTES {
                return Err("El índice de integridad es demasiado grande".into());
            }
            entry
                .read_to_string(&mut index)
                .map_err(|e| format!("No se pudo leer el índice: {e}"))?;
        }
        Err(zip::result::ZipError::FileNotFound) => return Ok(Audit::MissingIndex),
        Err(e) => return Err(e.to_string()),
    }
    let parsed: IntegrityToml =
        toml::from_str(&index).map_err(|e| format!("integrity.toml mal formado: {e}"))?;
    if parsed.entry.len() > MAX_ENTRY_COUNT {
        return Err("El índice excede el límite de entradas".into());
    }
    let mut listed_entries = BTreeMap::new();
    for entry in &parsed.entry {
        ayther_core::pack_security::validate_canonical_logical_path(&entry.path)
            .map_err(|e| e.to_string())?;
        if matches!(entry.path.as_str(), "integrity.toml" | "signature.bin")
            || listed_entries.insert(entry.path.as_str(), entry).is_some()
        {
            return Err("El índice repite una entrada o incluye una ruta reservada".into());
        }
    }
    let total = parsed
        .entry
        .iter()
        .map(|e| e.size)
        .try_fold(0u64, |sum, n| sum.checked_add(n))
        .ok_or("Tamaño de índice inválido")?;
    let mut names = std::collections::HashSet::new();
    let mut visited_entries = 0usize;
    let mut done = 0u64;
    let mut buffer = vec![0u8; 1024 * 1024];
    progress(0, total);
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("entrada {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(format!("Entrada repetida: {name}"));
        }
        if name == "integrity.toml" || name == "signature.bin" {
            continue;
        }
        let Some(e) = listed_entries.get(name.as_str()) else {
            return Ok(Audit::Mismatch(format!("«{name}» no está en el índice")));
        };
        if e.size != entry.size() || e.size > MAX_ENTRY_BYTES {
            return Ok(Audit::Mismatch(format!(
                "«{name}» mide {} y el índice dice {}",
                entry.size(),
                e.size
            )));
        }
        let mut hasher = Sha256::new();
        let mut entry_bytes = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Comprobación cancelada".into());
            }
            let n = entry
                .read(&mut buffer)
                .map_err(|e| format!("leyendo {name}: {e}"))?;
            if n == 0 {
                break;
            }
            entry_bytes += n as u64;
            if entry_bytes > e.size {
                return Ok(Audit::Mismatch(format!(
                    "«{name}» excede su tamaño declarado"
                )));
            }
            hasher.update(&buffer[..n]);
            done += n as u64;
            crate::cache_metrics::PACK_AUDIT_BYTES.fetch_add(n as u64, Ordering::Relaxed);
            progress(done, total);
        }
        if entry_bytes != e.size {
            return Ok(Audit::Mismatch(format!("«{name}» está incompleto")));
        }
        let hash = format!("{:x}", hasher.finalize());
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

    crate::cache_metrics::PACK_AUDIT_SUCCESSES.fetch_add(1, Ordering::Relaxed);
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
