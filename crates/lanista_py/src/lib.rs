//! Python module for RL training (phase 7): the learning environment of
//! `lanista_core::learning`.

#[pyo3::pymodule]
mod lanista_py {
    use lanista_core::design_data::DesignData;
    use lanista_core::learning;
    use pyo3::exceptions::{PyRuntimeError, PyValueError};
    use pyo3::prelude::*;

    /// A standard duel whose two fighters are played from Python.
    #[pyclass]
    struct Environment(learning::Environment);

    #[pymethods]
    impl Environment {
        /// Reads the design data from the source tree's `data/`.
        #[new]
        fn new() -> PyResult<Self> {
            let data = DesignData::load().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Self(learning::Environment::new(data)))
        }

        /// Starts the duel of `seed`; gives each fighter's observation.
        fn reset(&mut self, seed: u64) -> Vec<Vec<f64>> {
            self.0.reset(seed)
        }

        /// Plays one action per fighter until the next decision; gives each fighter's
        /// observation and reward, and whether the fight is over.
        #[expect(
            clippy::needless_pass_by_value,
            reason = "PyO3 extracts a Python list into a Vec, not a slice"
        )]
        fn step(&mut self, actions: Vec<usize>) -> PyResult<(Vec<Vec<f64>>, Vec<f64>, bool)> {
            let fighters = self.0.arena().fighters.len();
            let count = self.0.action_count();
            if actions.len() != fighters || actions.iter().any(|&a| a >= count) {
                return Err(PyValueError::new_err(format!(
                    "expected {fighters} actions below {count}, got {actions:?}"
                )));
            }
            let step = self.0.step(&actions);
            Ok((step.observations, step.rewards, step.done))
        }

        #[getter]
        fn action_count(&self) -> usize {
            self.0.action_count()
        }

        #[getter]
        fn observation_size(&self) -> usize {
            self.0.observation_size()
        }

        /// The fight has a winner or no one left; false at the time limit.
        #[getter]
        fn over(&self) -> bool {
            self.0.arena().is_over()
        }

        #[getter]
        fn fighters(&self) -> usize {
            self.0.arena().fighters.len()
        }
    }
}
