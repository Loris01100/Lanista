//! `tract` loads the dummy model (written by `python/dummy_model.py`) and gives the expected output.

use std::path::Path;

use lanista_ia_onnx::infer;

const MODEL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/dummy.onnx");
const INPUT: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
// relu([1, 2, 3, 4] @ W + B), computed by hand.
const EXPECTED: [f32; 2] = [1.6, 0.0];

fn check(y: &[f32]) {
    assert_eq!(y.len(), EXPECTED.len());
    for (a, b) in y.iter().zip(EXPECTED) {
        assert!((a - b).abs() < 1e-6, "{y:?} != {EXPECTED:?}");
    }
}

#[test]
fn loads_and_infers() {
    check(&infer(Path::new(MODEL), &INPUT).unwrap());
}
