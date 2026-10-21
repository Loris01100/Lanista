//! ONNX inference with `tract`, used by the tests only: the game plays the learned AI
//! from the networks `python/export.py` writes next to the ONNX models
//! (`lanista_core::learned`), the same on every processor.

use std::error::Error;
use std::path::Path;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Runs a model with one `[1, n]` float input and one float output.
///
/// # Errors
/// If the model cannot be loaded or run.
pub fn infer(model: &Path, input: &[f32]) -> BoxResult<Vec<f32>> {
    use tract_onnx::prelude::*;

    let model = tract_onnx::onnx()
        .model_for_path(model)?
        .into_optimized()?
        .into_runnable()?;
    let x = Tensor::from_shape(&[1, input.len()], input)?;
    let outputs = model.run(tvec!(x.into()))?;
    Ok(outputs[0]
        .to_plain_array_view::<f32>()?
        .iter()
        .copied()
        .collect())
}
