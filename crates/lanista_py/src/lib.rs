//! Python module for RL training (phase 7). For now, the phase 0 prototype only.

#[pyo3::pymodule]
mod lanista_py {
    use pyo3::prelude::*;

    #[pyfunction]
    fn addition(a: i64, b: i64) -> i64 {
        a + b
    }
}
