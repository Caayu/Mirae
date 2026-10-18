//! The harness itself against the real executable: start, read the state,
//! find widgets, draw, query pixels, close.

#![cfg(windows)]

use tela_e2e::{Session, sel, vk};

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn the_app_starts_draws_and_closes() {
    let mut s = Session::start("smoke");
    let st = s.settle();
    assert_eq!(st["tool"], "brush");
    assert_eq!(st["document"]["layers"].as_array().unwrap().len(), 1);
    assert!(s.widget(&sel("Undo")).is_ok());
    s.shot("start");
    // A brush stroke across the middle of the document.
    s.drag_doc([1000.0, 1500.0], [3000.0, 1500.0], 30);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "paint");
    assert_eq!(st["document"]["modified"], true);
    let (composite, layer) = s.pixel(2000, 1500);
    assert_eq!(composite, [0, 0, 0, 255], "the stroke is black on white");
    assert_eq!(layer, Some([0, 0, 0, 255]));
    let (paper, _) = s.pixel(2000, 500);
    assert_eq!(paper, [255, 255, 255, 255]);
    s.shot("stroke");
    s.key(&[vk::CONTROL], vk::letter('z'));
    let st = s.settle();
    assert_eq!(
        st["document"]["modified"], false,
        "undo returns to the start"
    );
    s.close("Don't Save");
    assert!(s.exited());
}
