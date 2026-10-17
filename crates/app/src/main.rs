//! Tela desktop application: the composition layer.
//!
//! **Responsibility:** the window and event loop (winit), wiring input
//! (Windows Ink + mouse) to the brush engine, committing strokes into the
//! document, recomposing dirty tiles, presenting through the renderer and
//! running the UI. It owns the authoritative application state
//! (document, view, tools/presets, user settings).
//! **Does not:** implement brush, composition or rendering logic itself.
//! **Depends on:** every other crate except corpus.

mod app;
mod colors;
mod selecting;
mod stats;
mod tools;

use app::{App, AppEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"),
    )
    .init();
    let event_loop = winit::event_loop::EventLoop::<AppEvent>::with_user_event().build()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
    let mut app = App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}
