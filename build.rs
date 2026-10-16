//! Embeds the Kenney sprites in `assets/kenney` for `--web`, under the short
//! names the page uses: `buildingTiles_001.png` is "b001", `cityTiles_066.png`
//! "c066", `cityDetails_010.png` "d010".

use std::{env, fs, path::Path};

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("set by cargo")).join("assets/kenney");
    println!("cargo:rerun-if-changed=assets/kenney");
    let mut entries = Vec::new();
    for (dir, prefix, short) in [
        ("buildings/PNG", "buildingTiles_", "b"),
        ("city/PNG", "cityTiles_", "c"),
        ("city/Details", "cityDetails_", "d"),
    ] {
        let Ok(files) = fs::read_dir(root.join(dir)) else { continue };
        for path in files.flatten().map(|file| file.path()) {
            let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
            if let Some(number) = stem.strip_prefix(prefix)
                && path.extension().is_some_and(|ext| ext == "png")
            {
                entries.push(format!("    (\"{short}{number}\", include_bytes!({:?})),", path.display().to_string()));
            }
        }
    }
    entries.sort();
    let code = format!(
        "/// Kenney sprites (CC0, kenney.nl) by the short name the page uses.\n\
         pub static SPRITES: &[(&str, &[u8])] = &[\n{}\n];\n",
        entries.join("\n")
    );
    let out = Path::new(&env::var("OUT_DIR").expect("set by cargo")).join("sprites.rs");
    fs::write(out, code).expect("cannot write sprites.rs");
}
