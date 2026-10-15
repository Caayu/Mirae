//! Prints a `.mirae` file's entries (name, method, sizes) and manifest.
//!
//! `cargo run -p tela-io --example mirae_dump -- <file.mirae>`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: mirae_dump <file.mirae>")?;
    let mut z = zip::ZipArchive::new(std::fs::File::open(&path)?)?;
    for i in 0..z.len() {
        let f = z.by_index(i)?;
        println!(
            "{:<40} {:?} {:>8} -> {:>8}",
            f.name()?,
            f.compression(),
            f.size(),
            f.compressed_size()
        );
    }
    let mut m = String::new();
    std::io::Read::read_to_string(&mut z.by_name("manifest.json")?, &mut m)?;
    let v: serde_json::Value = serde_json::from_str(&m)?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    let loaded = tela_io::load_document(path.as_ref(), &tela_io::Limits::default())?;
    println!(
        "loads: {} layers, warnings {:?}",
        loaded.document.layer_count(),
        loaded.warnings
    );
    Ok(())
}
