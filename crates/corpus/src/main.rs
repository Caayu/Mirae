//! Renders every built-in preset over the standard trajectories into a
//! directory, for inspection.
//!
//! ```sh
//! cargo run --release -p tela-corpus -- <output dir>
//! ```
//!
//! The output directory is required and must not be the golden directory:
//! references are only replaced through the explicit test-based update
//! (`TELA_UPDATE_GOLDEN=1 cargo test -p tela-corpus --test golden`, see
//! docs/golden-tests.md).

use std::path::PathBuf;

use tela_brush::preset::builtin_presets;
use tela_corpus::{golden_dir, golden_file_name, render_sheet};

fn main() -> std::io::Result<()> {
    let Some(dir) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: tela-corpus <output dir>   (goldens: see docs/golden-tests.md)");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&dir)?;
    if dir.canonicalize()? == golden_dir().canonicalize()? {
        eprintln!(
            "refusing to write into the golden directory; use \
             TELA_UPDATE_GOLDEN=1 cargo test -p tela-corpus --test golden"
        );
        std::process::exit(2);
    }
    for preset in builtin_presets() {
        let file = dir.join(golden_file_name(&preset));
        render_sheet(&preset).save_png(&file)?;
        println!("{}", file.display());
    }
    Ok(())
}
