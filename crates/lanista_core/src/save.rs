//! Saves: the [`GameState`] as JSON, with a version number. An older save is
//! brought up to date one version at a time before it is read.

use serde::Serialize;
use serde_json::{Value, json};

use crate::design_data::DesignData;
use crate::state::GameState;

/// Version written in new saves. Raise it with any change to the saved types,
/// and add the migration from the previous version to [`MIGRATIONS`].
pub const VERSION: u64 = 10;

/// `MIGRATIONS[i]` turns a version `i + 1` save into a version `i + 2` one,
/// working on the JSON so the old types need not be kept.
type Migration = fn(&mut Value, &DesignData);
const MIGRATIONS: [Migration; 9] = [
    no_colosseums,
    title_since,
    no_challenges,
    no_rivals,
    no_market,
    rival_rosters,
    no_staff,
    no_doctor_or_recruiter,
    accuracy_from_speed,
];
const _: () = assert!(MIGRATIONS.len() as u64 == VERSION - 1);

/// Version 2 adds the colosseums; [`from_json`] opens them.
fn no_colosseums(save: &mut Value, _: &DesignData) {
    save["state"]["colosseums"] = Value::Array(Vec::new());
}

/// Version 3 dates each title: held since the week of the save, so every champion
/// gets its full time to defend it.
fn title_since(save: &mut Value, _: &DesignData) {
    let week = save["state"]["week"].clone();
    if let Some(colosseums) = save["state"]["colosseums"].as_array_mut() {
        for colosseum in colosseums {
            colosseum["title_since"] = week.clone();
        }
    }
}

/// Version 4 adds the residents' challenges, none pending.
fn no_challenges(save: &mut Value, _: &DesignData) {
    save["state"]["challenges"] = Value::Array(Vec::new());
}

/// Version 5 adds the rival clubs; [`from_json`] opens them with the starting money.
fn no_rivals(save: &mut Value, _: &DesignData) {
    save["state"]["rivals"] = Value::Array(Vec::new());
}

/// Version 6 adds the free agents, which [`from_json`] brings to their fewest, and
/// whether a fighter was observed, false when missing.
fn no_market(save: &mut Value, _: &DesignData) {
    save["state"]["free_agents"] = Value::Array(Vec::new());
}

/// A version 6 fighter as version 7 has it: weeks observed instead of whether it was
/// observed, and released rather than gone (a club fighter who left was sold or released,
/// which version 6 did not tell apart).
fn weeks_observed(fighter: &mut Value) {
    let observed = fighter
        .as_object_mut()
        .and_then(|f| f.remove("observed"))
        .and_then(|o| o.as_bool())
        .unwrap_or(false);
    fighter["observed_weeks"] = json!(u32::from(observed));
    if fighter["status"] == "Left" {
        fighter["status"] = json!("Released");
    }
}

/// Version 7 gives the rival clubs their own fighters: each colosseum's residents join
/// the club that owned their place (colosseum + place, in turn among the clubs of
/// `data`), and rankings, champions and challenges name them by club. The transfer list
/// and offers start empty; a fighter's record starts at 0 when missing.
fn rival_rosters(save: &mut Value, data: &DesignData) {
    let state = &mut save["state"];
    for list in ["fighters", "free_agents"] {
        for fighter in state[list].as_array_mut().into_iter().flatten() {
            weeks_observed(fighter);
        }
    }
    let money = json!(data.economy.starting_money);
    if !state["rivals"].is_array() {
        state["rivals"] = json!([]);
    }
    let mut rosters: Vec<Vec<Value>> = vec![Vec::new(); data.world.rivals.len().max(1)];
    let mut ids: Vec<Vec<Value>> = Vec::new();
    let clubs = rosters.len();
    for (c, colosseum) in state["colosseums"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let residents = colosseum["residents"].take();
        let mut here = Vec::new();
        for (r, fighter) in residents.as_array().into_iter().flatten().enumerate() {
            let club = (c + r) % clubs;
            here.push(json!({ "club": club, "fighter": rosters[club].len() }));
            let mut fighter = fighter.clone();
            weeks_observed(&mut fighter);
            rosters[club].push(fighter);
        }
        let rename = |contender: &mut Value| {
            if let Some(r) = contender.get("Resident").and_then(Value::as_u64)
                && let Some(id) = usize::try_from(r).ok().and_then(|r| here.get(r))
            {
                *contender = json!({ "Rival": id });
            }
        };
        for rating in colosseum["ranking"].as_array_mut().into_iter().flatten() {
            rename(&mut rating["contender"]);
        }
        rename(&mut colosseum["champion"]);
        if let Some(object) = colosseum.as_object_mut() {
            object.remove("residents");
        }
        ids.push(here);
    }
    let rivals = state["rivals"].as_array_mut().expect("an array, set above");
    for (k, roster) in rosters.into_iter().enumerate() {
        if rivals.len() <= k {
            rivals.push(json!({ "money": money }));
        }
        rivals[k]["fighters"] = Value::Array(roster);
    }
    for challenge in state["challenges"].as_array_mut().into_iter().flatten() {
        let id = challenge["colosseum"]
            .as_u64()
            .zip(challenge["resident"].as_u64())
            .and_then(|(c, r)| {
                ids.get(usize::try_from(c).ok()?)?
                    .get(usize::try_from(r).ok()?)
            });
        challenge["rival"] = id.cloned().unwrap_or(Value::Null);
    }
    if let Some(challenges) = state["challenges"].as_array_mut() {
        challenges.retain(|ch| !ch["rival"].is_null());
    }
    if !state["free_agents"].is_array() {
        state["free_agents"] = json!([]);
    }
    state["transfer_list"] = json!([]);
    state["sale_offers"] = json!([]);
    state["counter_offer"] = Value::Null;
}

/// Version 8 adds the club's staff, no one hired.
fn no_staff(save: &mut Value, _: &DesignData) {
    save["state"]["staff"] = json!({ "trainer": null });
}

/// Version 9 adds a doctor and a recruiter to the staff, neither hired.
fn no_doctor_or_recruiter(save: &mut Value, _: &DesignData) {
    let staff = &mut save["state"]["staff"];
    staff["doctor"] = Value::Null;
    staff["recruiter"] = Value::Null;
}

/// Version 10 adds accuracy to the stats: every fighter's trained and potential
/// accuracy start at its speed, on the same scale (30 in the standard duel for both).
fn accuracy_from_speed(save: &mut Value, _: &DesignData) {
    let state = &mut save["state"];
    for list in ["fighters", "free_agents"] {
        for fighter in state[list].as_array_mut().into_iter().flatten() {
            speed_as_accuracy(fighter);
        }
    }
    for rival in state["rivals"].as_array_mut().into_iter().flatten() {
        for fighter in rival["fighters"].as_array_mut().into_iter().flatten() {
            speed_as_accuracy(fighter);
        }
    }
}

fn speed_as_accuracy(fighter: &mut Value) {
    for stats in ["stats", "potential"] {
        if fighter[stats].is_object() {
            fighter[stats]["accuracy"] = fighter[stats]["speed"].clone();
        }
    }
}

#[derive(Serialize)]
struct Save<'a> {
    version: u64,
    state: &'a GameState,
}

/// Why a save could not be read.
#[derive(Debug)]
pub enum SaveError {
    /// Not JSON, or not the expected fields.
    Json(serde_json::Error),
    /// No version, or one this game does not know (a save from a newer game).
    Version(Option<u64>),
    /// A fighter of a class missing from the design data.
    UnknownClass(u32),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "{e}"),
            Self::Version(None) => write!(f, "no version"),
            Self::Version(Some(v)) => write!(f, "unknown version {v} (latest is {VERSION})"),
            Self::UnknownClass(c) => write!(f, "unknown class {c}"),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            Self::Version(_) | Self::UnknownClass(_) => None,
        }
    }
}

/// The game as a save of the current [`VERSION`].
///
/// # Panics
/// Never: every saved type serializes to JSON.
#[must_use]
pub fn to_json(state: &GameState) -> String {
    let save = Save {
        version: VERSION,
        state,
    };
    serde_json::to_string_pretty(&save).expect("a game state serializes to JSON")
}

/// Reads a save of any known version, migrating it first, and opens the colosseums
/// and rival clubs of `data` it does not have, and the free agents it lacks.
///
/// # Errors
/// If the text is not a save, its version is unknown, or a fighter's class is not in `data`.
pub fn from_json(text: &str, data: &DesignData) -> Result<GameState, SaveError> {
    let mut save: Value = serde_json::from_str(text).map_err(SaveError::Json)?;
    let version = save.get("version").and_then(Value::as_u64);
    let from = match version {
        Some(v @ 1..=VERSION) => usize::try_from(v - 1).map_err(|_| SaveError::Version(version))?,
        _ => return Err(SaveError::Version(version)),
    };
    for migrate in &MIGRATIONS[from..] {
        migrate(&mut save, data);
    }
    let mut state: GameState =
        serde_json::from_value(save["state"].take()).map_err(SaveError::Json)?;
    // A fighter of an unknown class would panic when sent to fight.
    let rivals = state.rivals.iter().flat_map(|r| &r.fighters);
    if let Some(f) = state
        .fighters
        .iter()
        .chain(rivals)
        .chain(&state.free_agents)
        .find(|f| data.class(f.class).is_none())
    {
        return Err(SaveError::UnknownClass(f.class.0));
    }
    state.open_world(data);
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::ClassId;
    use crate::state::{Decisions, next_week};

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    /// The game of `seed` after `weeks` weeks of suggested decisions.
    fn played(seed: u64, weeks: u32, data: &DesignData) -> GameState {
        let mut state = GameState::new(seed, data);
        for _ in 0..weeks {
            next_week(&mut state, &Decisions::default(), data);
        }
        state
    }

    #[test]
    fn a_saved_game_reloads_as_it_was_and_plays_on_the_same() {
        let data = data();
        let mut state = played(7, 12, &data);
        let mut reloaded = from_json(&to_json(&state), &data).unwrap();
        assert_eq!(reloaded, state);
        // The random generator was saved where it stood.
        for _ in 0..20 {
            let a = next_week(&mut state, &Decisions::default(), &data);
            let b = next_week(&mut reloaded, &Decisions::default(), &data);
            assert_eq!(a, b);
        }
        assert_eq!(reloaded, state);
    }

    #[test]
    fn unknown_versions_are_refused() {
        let data = data();
        let save = to_json(&GameState::new(7, &data));
        let newer = save.replacen("\"version\": 10", "\"version\": 11", 1);
        assert!(matches!(
            from_json(&newer, &data),
            Err(SaveError::Version(Some(11)))
        ));
        let none = save.replacen("\"version\": 10", "\"versio\": 10", 1);
        assert!(matches!(
            from_json(&none, &data),
            Err(SaveError::Version(None))
        ));
        assert!(matches!(from_json("{", &data), Err(SaveError::Json(_))));
    }

    #[test]
    fn edited_saves_cannot_break_the_game() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].class = ClassId(99);
        assert!(matches!(
            from_json(&to_json(&state), &data),
            Err(SaveError::UnknownClass(99))
        ));
        let mut state = GameState::new(7, &data);
        state.rivals[1].fighters[0].class = ClassId(98);
        assert!(matches!(
            from_json(&to_json(&state), &data),
            Err(SaveError::UnknownClass(98))
        ));
        let state = GameState::new(7, &data);
        let form = format!("\"form\": {}", state.fighters[0].form.value());
        let edited = to_json(&state).replacen(&form, "\"form\": 250", 1);
        let reloaded = from_json(&edited, &data).unwrap();
        assert_eq!(reloaded.fighters[0].form.value(), 100);
    }
}
