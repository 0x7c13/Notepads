use entities::ENTITIES;
use std::io::Write;
use std::{env, path::PathBuf};

fn main() {
    let out_dir: PathBuf = env::var("OUT_DIR").unwrap().parse().unwrap();

    // entity::lookup is handed just the inner entity name, like "amp" for
    // "&amp;"; we only match those with a trailing ";".
    //
    // entities::ENTITIES includes many both with and without a trailing ";".
    // Exclude those without, and then write to source only the name, without
    // the leading or trailing "&" or ";".
    let translated_entities = ENTITIES
        .iter()
        .filter(|e| e.entity.starts_with('&') && e.entity.ends_with(';'))
        .map(|e| (&e.entity[1..e.entity.len() - 1], e.characters))
        .collect::<Vec<_>>();

    // Generate a perfect hash map for O(1) lookup
    let out = std::fs::File::create(out_dir.join("entitydata.rs")).unwrap();
    let mut bw = std::io::BufWriter::new(out);

    writeln!(bw, "mod entitydata {{").unwrap();
    writeln!(bw).unwrap();

    write!(
        bw,
        "    pub static ENTITY_MAP: phf::Map<&'static str, &'static str> = "
    )
    .unwrap();

    let mut map = phf_codegen::Map::new();
    for (entity, characters) in &translated_entities {
        map.entry(*entity, format!("{:?}", characters));
    }
    writeln!(bw, "{};", map.build()).unwrap();

    writeln!(bw, "}}").unwrap();
}
