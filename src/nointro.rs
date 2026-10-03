//! nointro.rs — No-Intro DAT file parser.
//!
//! Parses the standard No-Intro XML DAT format and builds a CRC32-keyed
//! lookup table for canonical game names and regions.  The DAT files are
//! freely available from No-Intro (https://datomatic.no-intro.org) after
//! registration; Ayther never bundles or distributes them.
//!
//! ## DAT format (relevant subset)
//!
//! ```xml
//! <datafile>
//!   <header>
//!     <name>Sega - Mega Drive - Genesis</name>
//!   </header>
//!   <game name="Sonic The Hedgehog 2 (World) (Rev A)">
//!     <description>Sonic The Hedgehog 2 (World) (Rev A)</description>
//!     <rom name="Sonic The Hedgehog 2 (World) (Rev A).md"
//!          size="1048576"
//!          crc="24B5AA3D"
//!          sha1="…"
//!          md5="…"/>
//!   </game>
//!   <!-- … thousands more games … -->
//! </datafile>
//! ```
//!
//! ## Usage
//!
//! ```rust
//! let dat  = DatFile::load(Path::new("Sega - Mega Drive - Genesis.dat")).unwrap();
//! let info = dat.lookup("24B5AA3D");   // CRC32 in uppercase hex
//! println!("{} [{}]", info.name, info.region);
//! // → "Sonic The Hedgehog 2 (World) (Rev A) [WLD]"
//! ```

use quick_xml::{Reader, events::Event};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Metadata for one ROM, extracted from a No-Intro DAT.
#[derive(Debug, Clone)]
pub struct DatEntry {
    /// Canonical game name (including region + revision tags).
    pub name: String,
    /// Short region code derived from the name's parenthesised tags.
    /// Examples: "USA", "EUR", "JPN", "WLD", "BRA", "UNK".
    pub region: String,
}

/// A loaded No-Intro DAT file, indexed by CRC32 for O(1) lookup.
pub struct DatFile {
    /// Key: 8-character uppercase hex CRC32 (e.g. `"24B5AA3D"`).
    /// Value: game metadata from the DAT.
    entries: HashMap<String, DatEntry>,
    /// Platform / set name from the DAT's `<header><name>`.
    pub platform_name: String,
}

impl DatFile {
    /// Load and parse a No-Intro XML DAT from `path`.
    ///
    /// Returns `None` if the file cannot be opened or contains no parseable
    /// entries — not an error, just an absent / unrecognised DAT.
    pub fn load(path: &Path) -> Option<Self> {
        let file = std::fs::File::open(path).ok()?;
        let reader = std::io::BufReader::new(file);
        let dat = Self::parse(reader);
        eprintln!(
            "[nointro] Loaded '{}': {} entries",
            dat.platform_name,
            dat.len()
        );
        Some(dat)
    }

    /// Look up a ROM by its CRC32.
    ///
    /// `crc32` must be in uppercase hex (8 chars), as returned by
    /// `crc32fast` after `format!("{:08X}", …)`.  Returns `None` if the
    /// CRC32 is not in the DAT (unknown / unreleased / bad dump).
    pub fn lookup(&self, crc32: &str) -> Option<&DatEntry> {
        self.entries.get(&crc32.to_ascii_uppercase())
    }

    /// Number of entries in the DAT.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Nadie la llama: existe porque `len` sin `is_empty` es lo primero que
    /// clippy señala, y la convención vale más que el warning.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // ---- Internal -----------------------------------------------------------

    fn parse<R: BufRead>(reader: R) -> Self {
        let mut xml = Reader::from_reader(reader);
        xml.config_mut().trim_text(true);

        let mut entries = HashMap::new();
        let mut platform_name = String::new();
        let mut in_header = false;
        let mut current_name = String::new();
        let mut buf = Vec::new();

        loop {
            match xml.read_event_into(&mut buf) {
                // ---- Open tags -----------------------------------------------
                Ok(Event::Start(ref e)) => match e.name().as_ref() {
                    b"header" => in_header = true,
                    b"game" => {
                        current_name.clear();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"name" {
                                current_name =
                                    String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                            }
                        }
                    }
                    _ => {}
                },
                // ---- Self-closing tags (rom, header children) ----------------
                Ok(Event::Empty(ref e))
                    if e.name().as_ref() == b"rom" && !current_name.is_empty() =>
                {
                    let mut crc = String::new();
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"crc" {
                            crc = String::from_utf8_lossy(attr.value.as_ref()).to_ascii_uppercase();
                        }
                    }
                    // Zero-pad to 8 chars in case the DAT omits leading zeros.
                    while crc.len() < 8 {
                        crc.insert(0, '0');
                    }
                    if !crc.is_empty() {
                        let region = extract_region(&current_name);
                        entries.insert(
                            crc,
                            DatEntry {
                                name: current_name.clone(),
                                region,
                            },
                        );
                    }
                }
                // ---- Text (header/name) --------------------------------------
                // quick-xml 0.38 uses BytesText::as_ref() → &[u8].
                // No-Intro header names never contain XML entities, so raw
                // UTF-8 decoding is safe.
                Ok(Event::Text(ref e)) => {
                    if in_header
                        && platform_name.is_empty()
                        && let Ok(text) = std::str::from_utf8(e.as_ref())
                    {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            platform_name = trimmed.to_string();
                        }
                    }
                }
                // ---- Close tags ----------------------------------------------
                Ok(Event::End(ref e)) if e.name().as_ref() == b"header" => {
                    in_header = false;
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        DatFile {
            entries,
            platform_name,
        }
    }
}

// ---------------------------------------------------------------------------
// Region extraction
// ---------------------------------------------------------------------------

/// Extract a short region code from a No-Intro game name.
///
/// No-Intro names encode region in parentheses after the title, e.g.:
/// `"Sonic The Hedgehog 2 (World) (Rev A)"` → `"WLD"`.
fn extract_region(name: &str) -> String {
    // Collect all parenthesised tokens.
    let mut tokens: Vec<&str> = Vec::new();
    let mut rest = name;
    while let Some(start) = rest.find('(') {
        if let Some(end) = rest[start..].find(')') {
            tokens.push(&rest[start + 1..start + end]);
            rest = &rest[start + end + 1..];
        } else {
            break;
        }
    }

    // Each parenthesised token may be comma-separated: "(USA, Europe)".
    // Split by comma and match each part independently (first match wins).
    for tok in &tokens {
        for part in tok.split(',') {
            let t = part.trim();
            let code = match t {
                "USA" | "US" => "USA",
                "Europe" | "EUR" => "EUR",
                "Japan" | "JPN" => "JPN",
                "World" | "WLD" => "WLD",
                "Brazil" | "BRA" => "BRA",
                "Australia" | "AUS" => "AUS",
                "Korea" | "KOR" => "KOR",
                "China" | "CHN" => "CHN",
                "Spain" | "ESP" => "ESP",
                "France" | "FRA" => "FRA",
                "Germany" | "GER" => "GER",
                "Italy" | "ITA" => "ITA",
                "Netherlands" | "NLD" => "NLD",
                "Sweden" | "SWE" => "SWE",
                "Scandinavia" => "SCA",
                "Canada" => "CAN",
                "Asia" => "ASA",
                "Taiwan" => "TWN",
                _ => continue,
            };
            return code.to_string();
        } // inner for part
    }

    "UNK".to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_extraction() {
        assert_eq!(
            extract_region("Sonic The Hedgehog 2 (World) (Rev A)"),
            "WLD"
        );
        assert_eq!(extract_region("Sonic The Hedgehog (USA, Europe)"), "USA");
        assert_eq!(extract_region("Magical Taruruuto-kun (Japan)"), "JPN");
        assert_eq!(extract_region("Unknown Game"), "UNK");
    }

    #[test]
    fn parse_minimal_dat() {
        let xml = r#"<?xml version="1.0"?>
<datafile>
  <header><name>Test Platform</name></header>
  <game name="My Game (USA)">
    <rom name="My Game (USA).md" size="512" crc="DEADBEEF" sha1="abc" md5="def"/>
  </game>
  <game name="Another (Japan)">
    <rom name="Another (Japan).md" size="256" crc="0000CAFE" sha1="xyz" md5="uvw"/>
  </game>
</datafile>"#;

        let dat = DatFile::parse(std::io::BufReader::new(xml.as_bytes()));
        assert_eq!(dat.platform_name, "Test Platform");
        assert_eq!(dat.len(), 2);

        let e = dat.lookup("DEADBEEF").unwrap();
        assert_eq!(e.name, "My Game (USA)");
        assert_eq!(e.region, "USA");

        let e2 = dat.lookup("0000CAFE").unwrap();
        assert_eq!(e2.region, "JPN");

        assert!(dat.lookup("FFFFFFFF").is_none());
    }
}
