//! Renders every built-in preset over the standard trajectories.
//!
//! ```sh
//! cargo run --release -p tela-corpus -- [output dir]   # default: crates/corpus/golden
//! ```
//!
//! Writing into the default directory replaces the golden references; the
//! usual way to do that is `TELA_UPDATE_GOLDEN=1 cargo test -p tela-corpus
//! --test golden` (see docs/golden-tests.md). Pass another directory to
//! inspect sheets without touching the references.

use std::path::PathBuf;

use tela_brush::preset::builtin_presets;
use tela_corpus::{golden_dir, golden_file_name, render_sheet};

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(golden_dir);
    std::fs::create_dir_all(&dir)?;
    for preset in builtin_presets() {
        let file = dir.join(golden_file_name(&preset));
        render_sheet(&preset).save_png(&file)?;
        println!("{}", file.display());
    }
    Ok(())
}
