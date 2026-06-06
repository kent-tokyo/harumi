//! Smoke tests that run under `wasm-pack test --headless --chrome`.
//! Only compiled when the target is wasm32.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

// run_in_browser requires a running HTTP server; run_in_node is simpler for CI.
// wasm-pack test --headless --chrome uses the browser mode automatically.

/// Verify that Document::new() produces a non-empty, valid-header PDF in WASM.
#[wasm_bindgen_test]
fn wasm_document_new_and_save() {
    let bytes = harumi::Document::new((595.0, 842.0))
        .expect("Document::new should succeed under WASM")
        .save_to_bytes()
        .expect("save_to_bytes should succeed under WASM");

    assert!(!bytes.is_empty(), "PDF bytes must not be empty");
    assert!(
        bytes.starts_with(b"%PDF"),
        "output must start with %PDF header"
    );
}
