//! Los oráculos de exportar e importar (#620).

use super::*;

fn tmp(n: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("play-portar-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(saves: &Path, rel: &str, data: &[u8]) {
    let p = saves.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

/// **CA de #620**: el export es un archivo que se abre afuera. Un ZIP con el
/// árbol tal cual — nada de un formato propio que sólo nosotros leemos.
#[test]
fn the_export_is_a_zip_that_opens_without_play() {
    let root = tmp("export");
    let saves = root.join("saves");
    put(
        &saves,
        "golden-axe/perfil1/20260101T000000Z-aabbccdd.srm",
        b"la partida",
    );
    put(
        &saves,
        "sonic/perfil1/20260102T000000Z-11223344.srm",
        b"otra",
    );
    let cfg = root.join("play_config.toml");
    std::fs::write(&cfg, "rom_dir = 'x'").unwrap();

    let destination = root.join("mis-partidas.zip");
    let r = export(&saves, Some(&cfg), &destination).unwrap();
    assert_eq!(r.saves, 2);
    assert!(r.preferences);

    // Se abre con cualquier lector de ZIP, y adentro está el mismo árbol.
    let f = std::fs::File::open(&destination).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n == "saves/golden-axe/perfil1/20260101T000000Z-aabbccdd.srm"),
        "{names:?}"
    );
    assert!(names.iter().any(|n| n == "play_config.toml"));
    let _ = std::fs::remove_dir_all(&root);
}

/// **La regla**: importar AGREGA. Lo que había sigue estando, y lo que viene con
/// el mismo nombre y distinto contenido entra al lado — no encima.
#[test]
fn importing_adds_and_never_stomps() {
    let root = tmp("importar");
    let origin = root.join("origen");
    let destination = root.join("destino");
    let rel = "golden-axe/perfil1/20260101T000000Z-aabbccdd.srm";

    put(&origin, rel, b"la de la otra PC");
    put(&destination, rel, b"la de ESTA maquina");

    let zip = root.join("export.zip");
    export(&origin, None, &zip).unwrap();

    let r = import(&zip, &destination, "20260829T120000Z").unwrap();
    assert_eq!(r.saves, 1);

    // La de esta máquina, intacta.
    assert_eq!(
        std::fs::read(destination.join(rel)).unwrap(),
        b"la de ESTA maquina"
    );
    // Y la importada, al lado y distinguible.
    let dir = destination.join("golden-axe/perfil1");
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    let imported_file = names
        .iter()
        .find(|n| n.contains("importado"))
        .expect("{names:?}");
    assert!(
        imported_file.contains("20260829T120000Z"),
        "con su sello: {imported_file}"
    );
    assert_eq!(
        std::fs::read(dir.join(imported_file)).unwrap(),
        b"la de la otra PC"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Importar dos veces el mismo export no duplica nada: lo idéntico se cuenta y
/// se saltea.
#[test]
fn importing_twice_does_not_duplicate() {
    let root = tmp("dosveces");
    let origin = root.join("origen");
    let destination = root.join("destino");
    put(&origin, "ga/p1/20260101T000000Z-aabbccdd.srm", b"igual");
    let zip = root.join("e.zip");
    export(&origin, None, &zip).unwrap();

    let a = import(&zip, &destination, "20260829T120000Z").unwrap();
    assert_eq!((a.saves, a.already_present), (1, 0));
    let b = import(&zip, &destination, "20260829T130000Z").unwrap();
    assert_eq!(
        (b.saves, b.already_present),
        (0, 1),
        "la segunda vez ya estaba"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Un ZIP es un archivo que trajo otra persona: una ruta que sale del árbol
/// —`../../`— se saltea. Es el ataque más viejo que hay contra un importador.
#[test]
fn a_zip_cannot_write_outside_the_tree() {
    let root = tmp("traversal");
    let destination = root.join("destino");
    std::fs::create_dir_all(&destination).unwrap();
    let zip_path = root.join("malo.zip");
    {
        let f = std::fs::File::create(&zip_path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        z.start_file("saves/../../afuera.srm", o).unwrap();
        z.write_all(b"no deberia estar").unwrap();
        // Y la configuración tampoco se importa sola: cambiar las preferencias
        // de alguien porque trajo sus partidas sería una sorpresa.
        z.start_file("play_config.toml", o).unwrap();
        z.write_all(b"rom_dir = 'del otro'").unwrap();
        z.finish().unwrap();
    }
    let r = import(&zip_path, &destination, "20260829T120000Z").unwrap();
    assert_eq!(r.saves, 0);
    assert_eq!(r.skipped, 2, "los dos se saltearon y se contaron");
    assert!(!root.join("afuera.srm").exists(), "nada fuera del árbol");
    assert!(!destination.join("play_config.toml").exists());
    let _ = std::fs::remove_dir_all(&root);
}
