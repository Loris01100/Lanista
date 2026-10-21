//! Python module for RL training (phase 7): the learning environment of
//! `lanista_core::learning`.

#[pyo3::pymodule]
mod lanista_py {
    use lanista_core::design_data::DesignData;
    use lanista_core::fighter::Fighter;
    use lanista_core::{learning, log};
    use pyo3::exceptions::{PyRuntimeError, PyValueError};
    use pyo3::prelude::*;

    /// A duel or a melee whose fighters are played from Python, with the
    /// classes' keys.
    #[pyclass]
    struct Environment(learning::Environment, Vec<String>);

    #[pymethods]
    impl Environment {
        /// Reads the design data from the source tree's `data/`.
        #[new]
        fn new() -> PyResult<Self> {
            let data = DesignData::load().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            let keys = data.classes.iter().map(|c| c.key.clone()).collect();
            Ok(Self(learning::Environment::new(data), keys))
        }

        /// Starts the duel of `seed`; gives each fighter's observation.
        fn reset(&mut self, seed: u64) -> Vec<Vec<f64>> {
            self.0.reset(seed)
        }

        /// Starts the melee of `seed` with `count` generated fighters.
        fn reset_melee(&mut self, seed: u64, count: u32) -> Vec<Vec<f64>> {
            self.0.reset_melee(seed, count)
        }

        /// Plays one action per fighter (`None` for the utility AI) until the next
        /// decision; gives each fighter's observation and reward, and whether the fight
        /// is over.
        #[expect(
            clippy::needless_pass_by_value,
            reason = "PyO3 extracts a Python list into a Vec, not a slice"
        )]
        fn step(
            &mut self,
            actions: Vec<Option<usize>>,
        ) -> PyResult<(Vec<Vec<f64>>, Vec<f64>, bool)> {
            let fighters = self.0.arena().fighters.len();
            let count = self.0.action_count();
            if actions.len() != fighters || actions.iter().flatten().any(|&a| a >= count) {
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

        /// Whether each fighter is still in the fight.
        #[getter]
        fn fighting(&self) -> Vec<bool> {
            let fighters = &self.0.arena().fighters;
            fighters.iter().map(Fighter::is_fighting).collect()
        }

        /// Every class's key, in the order of `data/classes.ron`.
        #[getter]
        fn class_keys(&self) -> Vec<String> {
            self.1.clone()
        }

        /// Each fighter's class key.
        #[getter]
        fn classes(&self) -> Vec<String> {
            let fighters = &self.0.arena().fighters;
            fighters
                .iter()
                .map(|f| self.1[usize::try_from(f.class.0).expect("a class index")].clone())
                .collect()
        }

        /// The fight so far as `simulate` prints it: its log, then its report.
        fn log(&self) -> String {
            let arena = self.0.arena();
            format!("{}\n{}", log::log_text(arena), log::Report::new(arena))
        }
    }
}
