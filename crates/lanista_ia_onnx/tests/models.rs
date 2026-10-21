//! The shipped models (`assets/ai/`, written by `python/export.py`) fit the learning
//! environment: each class has a recruit and a veteran model, taking its observation and
//! giving one logit per action.

use std::path::Path;

use lanista_core::design_data::DesignData;
use lanista_core::learned::Model;
use lanista_core::learning::{Environment, Senses};
use lanista_ia_onnx::infer;

#[test]
fn every_class_has_a_recruit_and_a_veteran_model_fitting_the_environment() {
    let data = DesignData::load().unwrap();
    let keys: Vec<String> = data.classes.iter().map(|c| c.key.clone()).collect();
    let env = Environment::new(data);
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/ai");
    let observation = vec![0.0; env.observation_size()];
    for key in keys {
        for level in ["recruit", "veteran"] {
            let logits = infer(&assets.join(format!("{key}-{level}.onnx")), &observation)
                .unwrap_or_else(|e| panic!("{key}-{level}: {e}"));
            assert_eq!(logits.len(), env.action_count(), "{key}-{level}");
        }
    }
}

/// `python/export.py`'s JSON of a model (here a placeholder's, written from its ONNX
/// by `export.network`) plays as the ONNX model does: `lanista_core::learned` gives
/// tract's logits on observations spread over -1..1.
#[test]
fn the_exported_network_gives_the_onnx_model_logits() {
    let data = DesignData::load().unwrap();
    let senses = Senses::of(&data);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(dir.join("tests/mirmillon-recruit.json")).unwrap();
    let model = Model::new(serde_json::from_str(&text).unwrap(), senses).unwrap();
    let onnx = dir.join("../../assets/ai/mirmillon-recruit.onnx");
    for seed in 0..20_u32 {
        let observation: Vec<f64> = (0..senses.observation_size())
            .map(|i| f64::from((u32::try_from(i).unwrap() * 37 + seed * 101) % 201) / 100.0 - 1.0)
            .collect();
        #[expect(clippy::cast_possible_truncation, reason = "the ONNX model takes f32")]
        let input: Vec<f32> = observation.iter().map(|&v| v as f32).collect();
        let expected = infer(&onnx, &input).unwrap();
        let logits = model.network.logits(&observation);
        assert_eq!(logits.len(), expected.len());
        for (a, b) in logits.iter().zip(expected) {
            assert!((a - f64::from(b)).abs() < 1e-4, "{a} against {b}");
        }
    }
}
