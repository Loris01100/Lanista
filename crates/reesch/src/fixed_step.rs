//! Fixed-rate logic loop: turns variable frame times into a whole number of
//! logic ticks, plus an interpolation factor for rendering between two ticks.

/// Longest frame time taken into account, in seconds. After a stall (window
/// dragged, breakpoint) the logic slows down instead of running hundreds of
/// ticks in one frame.
const MAX_FRAME_TIME: f64 = 0.25;

/// Accumulates real time and spends it in fixed logic ticks.
#[derive(Debug, Clone)]
pub struct FixedStep {
    tick_duration: f64,
    accumulator: f64,
}

impl FixedStep {
    /// # Panics
    /// If `ticks_per_second` is 0.
    #[must_use]
    pub fn new(ticks_per_second: u32) -> Self {
        assert!(ticks_per_second > 0, "ticks_per_second must be positive");
        Self {
            tick_duration: 1.0 / f64::from(ticks_per_second),
            accumulator: 0.0,
        }
    }

    /// Adds the time of the last frame, in seconds, and returns how many
    /// logic ticks to run now.
    pub fn advance(&mut self, frame_time: f64) -> u32 {
        self.accumulator += frame_time.clamp(0.0, MAX_FRAME_TIME);
        let mut ticks = 0;
        while self.accumulator >= self.tick_duration {
            self.accumulator -= self.tick_duration;
            ticks += 1;
        }
        ticks
    }

    /// Fraction of a tick elapsed since the last one, in `[0, 1)`: draw at
    /// `previous + (current - previous) * alpha`.
    #[must_use]
    pub fn alpha(&self) -> f64 {
        self.accumulator / self.tick_duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_fps_runs_one_tick_every_two_frames() {
        let mut step = FixedStep::new(30);
        let ticks: Vec<u32> = (0..6).map(|_| step.advance(1.0 / 60.0)).collect();
        // Float rounding may shift which frame ticks, never the total.
        assert_eq!(ticks.iter().sum::<u32>(), 3);
        assert!(ticks.iter().all(|&t| t <= 1));
    }

    #[test]
    fn alpha_is_the_leftover_fraction() {
        let mut step = FixedStep::new(30);
        assert_eq!(step.advance(1.5 / 30.0), 1);
        assert!((step.alpha() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_stall_is_capped() {
        let mut step = FixedStep::new(30);
        assert_eq!(step.advance(10.0), 7); // 0.25 s at 30 ticks per second
        assert!(step.alpha() < 1.0);
    }

    #[test]
    fn negative_frame_time_is_ignored() {
        let mut step = FixedStep::new(30);
        assert_eq!(step.advance(-1.0), 0);
        assert!(step.alpha().abs() < f64::EPSILON);
    }
}
