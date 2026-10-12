//! `tract` loads the dummy model (written by `python/modele_factice.py`) and gives the expected output.

use std::path::Path;

use lanista_ia_onnx::inferer;

const MODELE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/factice.onnx");
const ENTREE: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
// relu([1, 2, 3, 4] @ W + B), computed by hand.
const ATTENDU: [f32; 2] = [1.6, 0.0];

fn verifier(y: &[f32]) {
    assert_eq!(y.len(), ATTENDU.len());
    for (a, b) in y.iter().zip(ATTENDU) {
        assert!((a - b).abs() < 1e-6, "{y:?} != {ATTENDU:?}");
    }
}

#[test]
fn charge_et_infere() {
    verifier(&inferer(Path::new(MODELE), &ENTREE).unwrap());
}
