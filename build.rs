//! Embeds the Kenney sprites in `assets/kenney` for `--web` as `data:` URIs,
//! ready to go into the page (one request instead of one per sprite, #22),
//! under the short names the page uses: `buildingTiles_001.png` is "b001",
//! `cityTiles_066.png` "c066", `cityDetails_010.png` "d010". Also embeds the
//! fonts in `assets/fonts` under their file names.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

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
                let png = fs::read(&path).expect("cannot read a sprite");
                entries.push(format!("    (\"{short}{number}\", \"data:image/png;base64,{}\"),", base64(&png)));
            }
        }
    }
    entries.sort();
    let code = format!(
        "/// Kenney sprites (CC0, kenney.nl) as `data:` URIs by the short name the page uses.\n\
         pub static SPRITES: &[(&str, &str)] = &[\n{}\n];\n",
        entries.join("\n")
    );
    let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo"));
    fs::write(out.join("sprites.rs"), code).expect("cannot write sprites.rs");

    let fonts = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("set by cargo")).join("assets/fonts");
    println!("cargo:rerun-if-changed=assets/fonts");
    let mut entries = Vec::new();
    for path in fs::read_dir(&fonts).expect("assets/fonts is missing").flatten().map(|file| file.path()) {
        if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && name.ends_with(".woff2")
        {
            entries.push(format!("    ({name:?}, include_bytes!({:?})),", path.display().to_string()));
        }
    }
    entries.sort();
    let code = format!(
        "/// Fonts (SIL Open Font License) by file name.\n\
         pub static FONTS: &[(&str, &[u8])] = &[\n{}\n];\n",
        entries.join("\n")
    );
    fs::write(out.join("fonts.rs"), code).expect("cannot write fonts.rs");
}

/// Standard base64 with padding; the only encoding the page needs.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (k, &b)| n | (b as u32) << (16 - 8 * k));
        for k in 0..4 {
            out.push(if k <= chunk.len() { ALPHABET[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}
