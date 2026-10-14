//! What the player reads, in French: fighters, injuries, illnesses, fights,
//! weeks, money and colosseums.

use lanista_core::colosseum::{ArenaKind, Colosseum, ColosseumId, Contender};
use lanista_core::design_data::DesignData;
use lanista_core::economy::{Flow, Transaction};
use lanista_core::fighter::career::{Activity, CareerFighter, FightSummary, Status, WeekReport};
use lanista_core::fighter::fame::FightResult;
use lanista_core::fighter::illness::{Illness, IllnessLevel};
use lanista_core::fighter::injury::{Injury, Location, Severity, Side};

/// Stat names, in `Stats::to_array` order.
pub const STAT_NAMES: [&str; 6] = ["PV", "AC", "AL", "VIT", "DC", "DL"];

/// How the game names the fighter at `index` in the club, until fighters have names.
pub fn fighter_name(index: usize, fighter: &CareerFighter, data: &DesignData) -> String {
    format!("{} n° {}", class_name(fighter, data), index + 1)
}

/// One word for a fighter's state, worst first.
pub fn condition_text(fighter: &CareerFighter) -> &'static str {
    match fighter.status {
        Status::Dead => "mort",
        Status::Retired => "retraité",
        Status::Active if !fighter.injuries.iter().all(Injury::is_permanent) => "blessé",
        Status::Active if fighter.illness.is_some() => "malade",
        Status::Active => "en forme",
    }
}

pub fn class_name(fighter: &CareerFighter, data: &DesignData) -> String {
    data.class(fighter.class)
        .map_or_else(|| "?".into(), |c| c.name.clone())
}

pub fn activity_name(activity: Activity, data: &DesignData) -> String {
    match activity {
        Activity::Rest => "Repos".into(),
        Activity::Train => "Entraînement".into(),
        Activity::Fight(id) => format!("Combat à {}", colosseum_name(id, data)),
    }
}

pub fn colosseum_name(id: ColosseumId, data: &DesignData) -> &str {
    data.world.colosseum(id).map_or("?", |c| c.name.as_str())
}

pub fn arena_name(arena: ArenaKind) -> &'static str {
    match arena {
        ArenaKind::Sand => "sable",
        ArenaKind::Pillars => "piliers",
        ArenaKind::Narrow => "étroite",
    }
}

/// How a colosseum's ranking names `contender`; the club's fighters are marked.
pub fn contender_name(
    contender: Contender,
    colosseum: &Colosseum,
    club: &[CareerFighter],
    data: &DesignData,
) -> String {
    match contender {
        Contender::Club(i) => club.get(i).map_or_else(
            || "?".into(),
            |f| format!("{} (votre club)", fighter_name(i, f, data)),
        ),
        Contender::Resident(i) => colosseum.residents.get(i).map_or_else(
            || "?".into(),
            |f| format!("Résident n° {} ({})", i + 1, class_name(f, data)),
        ),
    }
}

pub fn location_name(location: Location, dominant: Side) -> String {
    let side = |side: Side, feminine: bool| match (side, feminine) {
        (Side::Left, _) => "gauche",
        (Side::Right, false) => "droit",
        (Side::Right, true) => "droite",
    };
    match location {
        Location::Head => "Tête".into(),
        Location::Body => "Corps".into(),
        Location::Arm(s) if s == dominant => format!("Bras {} (dominant)", side(s, false)),
        Location::Arm(s) => format!("Bras {}", side(s, false)),
        Location::Leg(s) => format!("Jambe {}", side(s, true)),
    }
}

pub fn weeks(n: u32) -> String {
    if n == 1 {
        "1 semaine".into()
    } else {
        format!("{n} semaines")
    }
}

pub fn injury_text(injury: Injury, dominant: Side) -> String {
    let location = location_name(injury.location, dominant);
    let severity = match injury.severity {
        Severity::Light => "légère",
        Severity::Medium => "moyenne",
        Severity::Severe => "grave",
        Severity::Permanent => return format!("{location} : séquelle définitive"),
    };
    format!("{location} : {severity}, {}", weeks(injury.weeks_left))
}

pub fn illness_text(illness: Illness) -> String {
    let level = match illness.level {
        IllnessLevel::Mild => "faible",
        IllnessLevel::Moderate => "moyenne",
        IllnessLevel::Severe => "sévère",
    };
    format!("Maladie {level}, {}", weeks(illness.weeks_left))
}

pub fn fight_text(fight: &FightSummary, data: &DesignData) -> String {
    let result = match (fight.result, fight.died) {
        (_, true) => "mort au combat",
        (FightResult::Won, _) => "victoire",
        (FightResult::Lost, _) => "défaite",
        (FightResult::GaveUp, _) => "abandon",
        (FightResult::Unfinished, _) => "pas de vainqueur après une minute",
    };
    let stronger = if fight.against_stronger {
        ", plus fort"
    } else {
        ""
    };
    format!(
        "Contre un {}{stronger} : {result}, renommée {:+}",
        class_name(&fight.opponent, data),
        fight.fame_change
    )
}

/// `name after (±difference)`, or `name value` when it did not change. Plain ASCII
/// signs: the theme's font has no arrow or minus sign.
pub fn change(name: &str, before: u32, after: u32) -> String {
    if before == after {
        format!("{name} {after}")
    } else {
        format!(
            "{name} {after} ({:+})",
            i64::from(after) - i64::from(before)
        )
    }
}

/// The club's money after the week and why it moved: `Argent 530 (+30) : recettes +20, …`.
pub fn money_line(before: i64, after: i64, ledger: &[Transaction]) -> String {
    let mut line = if before == after {
        format!("Argent {after}")
    } else {
        format!("Argent {after} ({:+})", after - before)
    };
    let moves: Vec<String> = ledger
        .iter()
        .map(|t| {
            let name = match t.flow {
                Flow::Upkeep => "entretien",
                Flow::Care => "soins",
                Flow::Training => "entraînement",
                Flow::EntryFee => "inscription",
                Flow::WinPrize => "prime de victoire",
                Flow::Receipts => "recettes",
            };
            format!("{name} {:+}", t.amount)
        })
        .collect();
    if !moves.is_empty() {
        line = format!("{line} : {}", moves.join(", "));
    }
    line
}

/// What a week changed, in the player's language.
pub fn week_lines(
    before: &CareerFighter,
    after: &CareerFighter,
    report: &WeekReport,
    data: &DesignData,
) -> Vec<String> {
    let dominant = after.dominant_hand;
    let mut lines = vec![activity_name(report.plan.activity, data)];
    if let Some(fight) = &report.fight {
        lines.push(fight_text(fight, data));
    }
    if let Some(injury) = &report.new_injury {
        lines.push(format!("Blessure : {}", injury_text(*injury, dominant)));
    }
    for injury in &report.healed_injuries {
        lines.push(format!(
            "Guéri : {}",
            location_name(injury.location, dominant)
        ));
    }
    if let Some(illness) = &report.new_illness {
        lines.push(format!("Tombe malade : {}", illness_text(*illness)));
    }
    if report.illness_healed {
        lines.push("Maladie guérie".into());
    }
    if report.birthday {
        lines.push(format!("Un an de plus : {} ans", after.age));
    }
    lines.push(format!(
        "{}, {}, {}",
        change("Forme", before.form.value(), after.form.value()),
        change("moral", before.morale.value(), after.morale.value()),
        change("renommée", before.fame.value(), after.fame.value())
    ));
    let stats: Vec<String> = STAT_NAMES
        .iter()
        .zip(
            before
                .stats
                .to_array()
                .into_iter()
                .zip(after.stats.to_array()),
        )
        .filter(|(_, (b, a))| a != b)
        .map(|(name, (b, a))| format!("{name} {:+}", i64::from(a) - i64::from(b)))
        .collect();
    if !stats.is_empty() {
        lines.push(format!("Stats : {}", stats.join(", ")));
    }
    match after.status {
        Status::Active | Status::Dead => {}
        Status::Retired => lines.push(format!("Prend sa retraite à {} ans", after.age)),
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injuries_read_in_french() {
        let leg = Injury {
            location: Location::Leg(Side::Right),
            severity: Severity::Light,
            weeks_left: 1,
        };
        assert_eq!(
            injury_text(leg, Side::Right),
            "Jambe droite : légère, 1 semaine"
        );
        let arm = Injury {
            location: Location::Arm(Side::Left),
            severity: Severity::Permanent,
            weeks_left: 0,
        };
        assert_eq!(
            injury_text(arm, Side::Left),
            "Bras gauche (dominant) : séquelle définitive"
        );
        assert_eq!(
            location_name(Location::Arm(Side::Right), Side::Left),
            "Bras droit"
        );
    }
}
