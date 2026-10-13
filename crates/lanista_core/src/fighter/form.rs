//! Physical form, from 0 (exhausted) to 100 (fresh), and the risk formula of the design plan.

use serde::Deserialize;

/// Physical form of a fighter, always within 0 to [`Form::MAX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Form(u32);

impl Form {
    pub const MAX: u32 = 100;

    /// Form `value`, capped at [`Form::MAX`].
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value.min(Self::MAX))
    }

    #[must_use]
    pub fn full() -> Self {
        Self(Self::MAX)
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }

    /// Form after `event`, staying within 0 to [`Form::MAX`].
    #[must_use]
    pub fn after(self, event: FormEvent, rules: &FormRules) -> Self {
        match event {
            FormEvent::Fight => Self(self.0.saturating_sub(rules.fight)),
            FormEvent::TitleDefense => Self(self.0.saturating_sub(rules.title_defense)),
            FormEvent::Training => Self(self.0.saturating_sub(rules.training)),
            FormEvent::Rest => Self::new(self.0.saturating_add(rules.rest)),
        }
    }

    /// Chance of an injury or illness whose chance at full form is `base`:
    /// `base × (1 + k × (1 − form / 100))`, capped at 1.
    #[must_use]
    pub fn risk(self, base: f64, rules: &FormRules) -> f64 {
        let fatigue = 1.0 - f64::from(self.0) / f64::from(Self::MAX);
        (base * (1.0 + rules.risk_factor * fatigue)).min(1.0)
    }
}

/// What changes a fighter's form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormEvent {
    Fight,
    /// Costs more than a normal fight.
    TitleDefense,
    Training,
    Rest,
}

/// Form costs and gains, and the risk formula's `k`, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormRules {
    /// `k` of the risk formula: risk is multiplied by `1 + k` at form 0.
    pub risk_factor: f64,
    /// Form lost in a fight.
    pub fight: u32,
    /// Form lost in a title defense.
    pub title_defense: u32,
    /// Form lost in a week of training.
    pub training: u32,
    /// Form recovered in a week of rest.
    pub rest: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const RULES: FormRules = FormRules {
        risk_factor: 2.0,
        fight: 20,
        title_defense: 30,
        training: 5,
        rest: 15,
    };

    #[test]
    fn new_caps_at_max() {
        assert_eq!(Form::new(150), Form::full());
        assert_eq!(Form::new(40).value(), 40);
    }

    #[test]
    fn events_move_form_by_their_rule() {
        let form = Form::new(50);
        assert_eq!(form.after(FormEvent::Fight, &RULES).value(), 30);
        assert_eq!(form.after(FormEvent::TitleDefense, &RULES).value(), 20);
        assert_eq!(form.after(FormEvent::Training, &RULES).value(), 45);
        assert_eq!(form.after(FormEvent::Rest, &RULES).value(), 65);
    }

    #[test]
    fn form_stays_within_bounds() {
        assert_eq!(
            Form::new(10).after(FormEvent::TitleDefense, &RULES).value(),
            0
        );
        assert_eq!(Form::new(95).after(FormEvent::Rest, &RULES), Form::full());
    }

    #[test]
    fn risk_is_base_at_full_form_and_tripled_at_zero_with_k_2() {
        assert!((Form::full().risk(0.1, &RULES) - 0.1).abs() < 1e-12);
        assert!((Form::new(50).risk(0.1, &RULES) - 0.2).abs() < 1e-12);
        assert!((Form::new(0).risk(0.1, &RULES) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn risk_is_capped_at_1() {
        assert!((Form::new(0).risk(0.5, &RULES) - 1.0).abs() < f64::EPSILON);
    }

    proptest! {
        #[test]
        fn lower_form_never_lowers_risk(a in 0..=Form::MAX, b in 0..=Form::MAX, base in 0.0..=1.0f64) {
            let (low, high) = (Form::new(a.min(b)), Form::new(a.max(b)));
            prop_assert!(low.risk(base, &RULES) >= high.risk(base, &RULES));
        }

        #[test]
        fn risk_stays_a_probability(form in 0..=Form::MAX, base in 0.0..=1.0f64) {
            let risk = Form::new(form).risk(base, &RULES);
            prop_assert!((0.0..=1.0).contains(&risk));
        }
    }
}
