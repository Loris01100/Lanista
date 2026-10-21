//! The shipped models (`assets/ai/`, written by `python/export.py`) fit the learning
//! environment: each class has a recruit and a veteran model, taking its observation and
//! giving one logit per action.

use std::path::Path;

use lanista_core::design_data::DesignData;
use lanista_core::learning::Environment;
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
