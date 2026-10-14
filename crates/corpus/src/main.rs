//! Renders every built-in preset over the standard trajectories.
//!
//! ```sh
//! cargo run --release -p tela-corpus -- [output dir]   # default docs/phase3/corpus
//! ```

use std::path::PathBuf;

use tela_brush::preset::builtin_presets;
use tela_corpus::render_sheet;

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("docs/phase3/corpus"));
    for preset in builtin_presets() {
        let file = dir.join(format!("{}.png", preset.id.trim_start_matches("tela.")));
        render_sheet(&preset).save_png(&file)?;
        println!("{}", file.display());
    }
    Ok(())
}
