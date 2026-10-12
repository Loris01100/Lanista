//! ONNX inference for the learned combat AI (phase 7).
//!
//! Phase 0 prototype: the same model run with `ort` and with `tract`, to choose one.

use std::error::Error;
use std::path::Path;

type Resultat<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Runs a model with one `[1, n]` float input `x` and one float output `y`, using `ort`.
///
/// # Errors
/// If the model cannot be loaded or run.
pub fn inferer_ort(modele: &Path, entree: &[f32]) -> Resultat<Vec<f32>> {
    let mut session = ort::session::Session::builder()?.commit_from_file(modele)?;
    let x = ort::value::Tensor::from_array(([1, entree.len()], entree.to_vec()))?;
    let sorties = session.run(ort::inputs!["x" => x])?;
    let (_forme, y) = sorties["y"].try_extract_tensor::<f32>()?;
    Ok(y.to_vec())
}

/// Same as [`inferer_ort`], using `tract`.
///
/// # Errors
/// If the model cannot be loaded or run.
pub fn inferer_tract(modele: &Path, entree: &[f32]) -> Resultat<Vec<f32>> {
    use tract_onnx::prelude::*;

    let modele = tract_onnx::onnx()
        .model_for_path(modele)?
        .into_optimized()?
        .into_runnable()?;
    let x = Tensor::from_shape(&[1, entree.len()], entree)?;
    let sorties = modele.run(tvec!(x.into()))?;
    Ok(sorties[0]
        .to_plain_array_view::<f32>()?
        .iter()
        .copied()
        .collect())
}
