//! What the player reads, from the language files: fighters, injuries, illnesses, fights,
//! weeks, money, colosseums, the market and the staff.

use lanista_core::calendar::Date;
use lanista_core::colosseum::{
    Challenge, ColosseumData, ColosseumId, Contender, Continent, TitleNews, title_kept,
};
use lanista_core::combat::ArenaKind;
use lanista_core::design_data::DesignData;
use lanista_core::economy::{Flow, Transaction, Wealth};
use lanista_core::fighter::STAT_COUNT;
use lanista_core::fighter::care::CareRules;
use lanista_core::fighter::career::{Activity, CareerFighter, Record, Status};
use lanista_core::fighter::fame::FightResult;
use lanista_core::fighter::illness::{Illness, IllnessLevel};
use lanista_core::fighter::injury::{Injury, Location, Severity, Side};
use lanista_core::fighter::personality::Personality;
use lanista_core::journal::{FightLine, FighterWeek, Retired};
use lanista_core::market::{
    MarketNews, Move, Target, ease_seen, personality_seen, potential_range, stat_ranges,
};
use lanista_core::names::{ClubName, FighterName};
use lanista_core::relations::{Change, Reason, Standing};
use lanista_core::rivals::{Profile, RivalId};
use lanista_core::staff::{
    Candidate, DoctorRules, RecruiterRules, Role, Scouting, Staff, StaffMove, StaffNews,
    TrainerRules,
};
use lanista_core::state::{GameState, News, WorldNews};

use crate::lang::tr;

/// Stat names, in `Stats::to_array` order.
pub fn stat_names() -> [String; STAT_COUNT] {
    [
        tr!("stat-hp"),
        tr!("stat-melee-attack"),
        tr!("stat-ranged-attack"),
        tr!("stat-accuracy"),
        tr!("stat-speed"),
        tr!("stat-melee-defense"),
        tr!("stat-ranged-defense"),
    ]
}

/// "Flamma (Mirmillon)": how the game names the fighter at `index` in the club; by its
/// class and that index if it has no name.
pub fn fighter_name(index: usize, fighter: &CareerFighter, data: &DesignData) -> String {
    let class = class_name(fighter, data);
    if fighter.name.is_empty() {
        tr!("fighter-unnamed", class = class, number = index + 1)
    } else {
        tr!(
            "fighter-name",
            name = stage_name(&fighter.name),
            class = class
        )
    }
}

/// "Flamma II": a fighter's stage name, numbered when every name of its culture was
/// borne.
pub fn stage_name(name: &FighterName) -> String {
    if name.number < 2 {
        name.name.clone()
    } else {
        tr!(
            "fighter-numbered",
            name = name.name.as_str(),
            number = roman(name.number)
        )
    }
}

/// "Origine : Grèce (Europe)": the culture a fighter comes from and its continent;
/// `None` for one with no culture.
pub fn origin_text(fighter: &CareerFighter, data: &DesignData) -> Option<String> {
    let culture = data.culture(&fighter.culture)?;
    Some(tr!(
        "sheet-origin",
        culture = culture_name(&culture.key),
        continent = continent_name(culture.continent)
    ))
}

/// One word for a fighter's state, worst first.
pub fn condition_text(fighter: &CareerFighter) -> String {
    match fighter.status {
        Status::Dead => tr!("condition-dead"),
        Status::Retired => tr!("condition-retired"),
        Status::Sold => tr!("condition-sold"),
        Status::Released => tr!("condition-released"),
        Status::Active if !fighter.injuries.iter().all(Injury::is_permanent) => {
            tr!("condition-injured")
        }
        Status::Active if fighter.illness.is_some() => tr!("condition-ill"),
        Status::Active => tr!("condition-fit"),
    }
}

pub fn class_name(fighter: &CareerFighter, data: &DesignData) -> String {
    data.class(fighter.class)
        .map_or_else(|| "?".into(), |c| data_name("class", &c.key))
}

/// The name of loadout `loadout` of the fighter's class (`ClassData::loadout`).
pub fn loadout_name(fighter: &CareerFighter, loadout: usize, data: &DesignData) -> String {
    data.class(fighter.class)
        .and_then(|c| c.loadout(loadout))
        .map_or_else(|| "?".into(), |l| data_name("loadout", &l.key))
}

/// Its ease with each loadout of its class: "Glaive et bouclier 85, Double glaive 40".
pub fn ease_text(fighter: &CareerFighter, data: &DesignData) -> String {
    let loadouts = data.class(fighter.class).map_or(0, |c| c.loadouts.len());
    let each = (0..loadouts).map(|l| {
        let loadout = loadout_name(fighter, l, data);
        tr!(
            "ease-loadout",
            loadout = loadout,
            ease = fighter.ease_with(l)
        )
    });
    each.collect::<Vec<_>>().join(", ")
}

/// The ease of a fighter out of the club, if the recruiter sees it, or "?".
pub fn outside_ease_text(
    fighter: &CareerFighter,
    scouting: &Scouting,
    data: &DesignData,
) -> String {
    if ease_seen(fighter, scouting) {
        ease_text(fighter, data)
    } else {
        "?".into()
    }
}

pub fn activity_name(activity: Activity, data: &DesignData) -> String {
    match activity {
        Activity::Rest => tr!("activity-rest"),
        Activity::Train => tr!("activity-train"),
        Activity::Fight(id) => tr!("activity-fight", place = colosseum_name(id, data)),
        Activity::TitleFight(id) => tr!("activity-title-fight", place = colosseum_name(id, data)),
        Activity::Challenge(id, _) => tr!("activity-challenge", place = colosseum_name(id, data)),
        Activity::Melee(id) => tr!("activity-melee", place = colosseum_name(id, data)),
    }
}

/// "Flamma (Mirmillon, free agent)": how the market names the free agent at `index`.
pub fn free_agent_name(index: usize, fighter: &CareerFighter, data: &DesignData) -> String {
    let class = class_name(fighter, data);
    if fighter.name.is_empty() {
        tr!("free-agent-unnamed", number = index + 1, class = class)
    } else {
        tr!(
            "free-agent-name",
            name = stage_name(&fighter.name),
            class = class
        )
    }
}

/// "Flamma (Augure, Ludus Magnus)": how the game names a rival club's fighter; by its
/// class and place in the roster if it has no name.
pub fn rival_fighter_name(id: RivalId, state: &GameState, data: &DesignData) -> String {
    let fighter = state.rival_fighter(id);
    let class = fighter.map_or_else(|| "?".into(), |f| class_name(f, data));
    let club = club_name(id.club, state);
    match fighter {
        Some(f) if !f.name.is_empty() => tr!(
            "rival-fighter-name",
            name = stage_name(&f.name),
            class = class,
            club = club
        ),
        _ => tr!(
            "rival-fighter-unnamed",
            class = class,
            number = id.fighter + 1,
            club = club
        ),
    }
}

/// "40-60", or "?" for what is not seen.
fn range_text(range: Option<(u32, u32)>) -> String {
    range.map_or_else(|| "?".into(), |(low, high)| format!("{low}-{high}"))
}

/// What the club sees of each trained stat of a fighter out of it, with the
/// recruiter's `scouting`: "40-60", or "?".
pub fn range_texts(fighter: &CareerFighter, scouting: &Scouting, data: &DesignData) -> Vec<String> {
    stat_ranges(fighter, &data.economy.market, scouting)
        .into_iter()
        .map(range_text)
        .collect()
}

/// The recruiter's estimate of the total of a fighter's potential: "380-460", or "?".
pub fn potential_text(fighter: &CareerFighter, scouting: &Scouting) -> String {
    range_text(potential_range(fighter, scouting))
}

/// "Agressivité 60, prudence 40, goût du spectacle 50, détermination 70".
pub fn personality_text(p: &Personality) -> String {
    tr!(
        "personality",
        aggression = p.aggression.value(),
        caution = p.caution.value(),
        showmanship = p.showmanship.value(),
        determination = p.determination.value()
    )
}

/// The personality of a fighter out of the club, in short, if the recruiter sees it:
/// "Agr 60, Pru 40, Spe 50, Dét 70", or "?".
pub fn outside_personality_text(fighter: &CareerFighter, scouting: &Scouting) -> String {
    if !personality_seen(fighter, scouting) {
        return "?".into();
    }
    let p = &fighter.personality;
    tr!(
        "personality-short",
        aggression = p.aggression.value(),
        caution = p.caution.value(),
        showmanship = p.showmanship.value(),
        determination = p.determination.value()
    )
}

/// "3 V, 1 D, 2 N".
pub fn record_text(record: Record) -> String {
    tr!(
        "record",
        wins = record.wins,
        losses = record.losses,
        draws = record.draws
    )
}

/// The fighter out of the club `target` names.
fn target_name(target: Target, state: &GameState, data: &DesignData) -> String {
    match target {
        Target::FreeAgent(a) => state
            .free_agents
            .get(a)
            .map_or_else(|| "?".into(), |f| free_agent_name(a, f, data)),
        Target::Rival(id) => rival_fighter_name(id, state, data),
    }
}

fn club_fighter(i: usize, state: &GameState, data: &DesignData) -> String {
    state
        .fighters
        .get(i)
        .map_or_else(|| "?".into(), |f| fighter_name(i, f, data))
}

/// A market move chosen for the week, in one line.
pub fn move_text(m: Move, state: &GameState, data: &DesignData) -> String {
    let club = |i| club_fighter(i, state, data);
    match m {
        Move::Observe(target) => tr!("move-observe", fighter = target_name(target, state, data)),
        Move::Sign(a) => tr!(
            "move-sign",
            fighter = target_name(Target::FreeAgent(a), state, data)
        ),
        Move::Release(i) => tr!("move-release", fighter = club(i)),
        Move::List(i) => tr!("move-list", fighter = club(i)),
        Move::Unlist(i) => tr!("move-unlist", fighter = club(i)),
        Move::Keep(i) => tr!("move-keep", fighter = club(i)),
        Move::Unkeep(i) => tr!("move-unkeep", fighter = club(i)),
        Move::AcceptOffer { fighter, rival } => {
            let price = state
                .sale_offers
                .iter()
                .find(|o| o.fighter == fighter && o.rival == rival)
                .map_or(0, |o| o.price);
            tr!(
                "move-accept-offer",
                fighter = club(fighter),
                club = club_name(rival, state),
                price = price
            )
        }
        Move::Offer(id, price) => tr!(
            "move-offer",
            price = price,
            fighter = rival_fighter_name(id, state, data)
        ),
    }
}

/// The key a post is selected by in the language files.
fn role_key(role: Role) -> &'static str {
    match role {
        Role::Trainer => "trainer",
        Role::Doctor => "doctor",
        Role::Recruiter => "recruiter",
    }
}

/// The name of a post, as a title.
pub fn role_title(role: Role) -> String {
    tr!("role-title", role = role_key(role))
}

/// A club's staff: "Personnel : entraîneur de niveau 5, médecin de niveau 2", or
/// "Personnel : aucun".
pub fn staff_text(staff: &Staff) -> String {
    let posts: Vec<String> = Role::ALL
        .into_iter()
        .filter_map(|role| {
            let level = staff.level(role)?;
            Some(tr!("staff-post", role = role_key(role), level = level))
        })
        .collect();
    if posts.is_empty() {
        tr!("staff-none")
    } else {
        tr!("staff", posts = posts.join(", "))
    }
}

/// A change of staff, as chosen for the week among the candidates on `market`.
pub fn staff_move_text(m: StaffMove, market: &[Candidate]) -> String {
    match m {
        StaffMove::Hire(c) => match market.get(c) {
            Some(candidate) => {
                let person = &candidate.person;
                tr!(
                    "staff-move-hire",
                    role = role_key(person.role()),
                    level = person.level()
                )
            }
            None => tr!("staff-move-hire-gone"),
        },
        StaffMove::Dismiss(role) => tr!("staff-move-dismiss", role = role_key(role)),
    }
}

/// A change of staff, once made.
pub fn staff_news_text(news: StaffNews) -> String {
    match news {
        StaffNews::Hired(role, level) => {
            tr!("staff-news-hired", role = role_key(role), level = level)
        }
        StaffNews::Dismissed(role) => tr!("staff-news-dismissed", role = role_key(role)),
    }
}

/// How long a candidate stays on the market: "encore 3 semaines", or "dernière semaine".
pub fn weeks_left_text(weeks: u32) -> String {
    tr!("weeks-left", n = weeks)
}

/// What a trainer costs and brings: "salaire 20 par semaine, séance 6, progression
/// +20 %, 1 de forme épargnée par séance".
pub fn trainer_text(trainer: &TrainerRules) -> String {
    tr!(
        "trainer",
        salary = trainer.salary,
        session = trainer.training_cost,
        progress = trainer.coaching.progress_percent,
        form = trainer.coaching.form_spared
    )
}

/// What a doctor costs and brings: "salaire 20 par semaine, les soins laissent 40 %
/// des semaines (50 sans médecin), prix des soins 110 %, séquelle évitée 30 fois sur 100".
pub fn doctor_text(doctor: &DoctorRules, care: &CareRules) -> String {
    let medicine = &doctor.medicine;
    tr!(
        "doctor",
        salary = doctor.salary,
        kept = medicine.care_rules(care).weeks_kept_percent,
        kept_alone = care.weeks_kept_percent,
        cost = medicine.care_cost_percent,
        spared = medicine.after_effect_spared_percent
    )
}

/// What a recruiter costs and brings: "salaire 30 par semaine, 1 stat de plus
/// révélée par semaine, fourchettes à 70 %, potentiel estimé à 40 % près, personnalité
/// cachée, aisance vue".
pub fn recruiter_text(recruiter: &RecruiterRules) -> String {
    let scouting = &recruiter.scouting;
    tr!(
        "recruiter",
        salary = recruiter.salary,
        revealed = scouting.revealed_per_week,
        width = scouting.range_width_percent,
        potential = scouting.potential_range_percent,
        personality = if scouting.personality {
            "seen"
        } else {
            "hidden"
        },
        ease = if scouting.ease { "seen" } else { "hidden" }
    )
}

/// What happened on the market, in one line, from the game after the week.
pub fn market_news_text(news: MarketNews, state: &GameState, data: &DesignData) -> String {
    let club = |i| club_fighter(i, state, data);
    let rival = |id| rival_fighter_name(id, state, data);
    match news {
        MarketNews::Signed(i) => tr!("market-signed", fighter = club(i)),
        MarketNews::Released(i) => tr!("market-released", fighter = club(i)),
        MarketNews::Sold(offer) => tr!(
            "market-sold",
            fighter = club(offer.fighter),
            club = club_name(offer.rival, state),
            price = offer.price
        ),
        MarketNews::Bought {
            from,
            fighter,
            price,
        } => tr!(
            "market-bought",
            fighter = club(fighter),
            club = club_name(from.club, state),
            price = price
        ),
        MarketNews::Countered(id, price) => tr!(
            "market-countered",
            club = club_name(id.club, state),
            price = price,
            fighter = rival(id)
        ),
        MarketNews::ClubRefused(id) => tr!(
            "market-club-refused",
            club = club_name(id.club, state),
            fighter = rival(id)
        ),
        MarketNews::FighterRefused(id) => tr!("market-fighter-refused", fighter = rival(id)),
        MarketNews::Failed(id) => {
            let fighter = rival(id);
            tr!("market-failed", vowel = vowel(&fighter), fighter = fighter)
        }
    }
}

/// "Augure n° 3 (Ludus Magnus) à Capoue": who sent or would receive the challenge, and
/// where.
pub fn challenger_text(challenge: Challenge, state: &GameState, data: &DesignData) -> String {
    tr!(
        "challenger",
        fighter = rival_fighter_name(challenge.rival, state, data),
        place = colosseum_name(challenge.colosseum, data)
    )
}

/// What happened to the title of the colosseum at index `c`, in one line.
pub fn title_news_text(c: usize, news: TitleNews, state: &GameState, data: &DesignData) -> String {
    let Some(rules) = data.world.colosseums.get(c) else {
        return String::new();
    };
    let name = |contender| contender_name(contender, state, data);
    let to = |to: Option<Contender>| to.map_or_else(|| tr!("title-nobody"), name);
    let place = colosseum_rules_name(rules);
    match news {
        TitleNews::Fight {
            champion,
            challenger,
            result,
        } => tr!(
            "title-fight",
            place = place.as_str(),
            champion = name(champion),
            kept = if title_kept(result) { "true" } else { "false" },
            challenger = name(challenger)
        ),
        TitleNews::Stripped { former, to: holder } => tr!(
            "title-stripped",
            place = place.as_str(),
            former = name(former),
            holder = to(holder)
        ),
        TitleNews::Vacated { to: holder } => {
            tr!("title-vacated", place = place.as_str(), holder = to(holder))
        }
    }
}

/// A world news item in one line, dated: "Saison 2, semaine 7 : Ludus Magnus ferme,
/// ruiné : ses combattants partent au marché".
pub fn world_news_text(news: News, state: &GameState, data: &DesignData) -> String {
    let what = match news.what {
        WorldNews::Title(c, title) => title_news_text(c, title, state, data),
        WorldNews::ChampionBought {
            colosseum,
            fighter,
            buyer,
            price,
        } => tr!(
            "news-champion-bought",
            club = club_name(buyer, state),
            fighter = rival_fighter_name(fighter, state, data),
            place = data
                .world
                .colosseums
                .get(colosseum)
                .map_or_else(|| "?".into(), colosseum_rules_name),
            price = price
        ),
        WorldNews::Closed(club) => tr!("news-closed", club = club_name(club, state)),
        WorldNews::Rivalry(club, true) => tr!("news-rivalry", club = club_name(club, state)),
        WorldNews::Rivalry(club, false) => {
            tr!("news-rivalry-ended", club = club_name(club, state))
        }
        WorldNews::Founded(club) => state.rivals.get(club).map_or_else(String::new, |r| {
            tr!(
                "news-founded",
                club = named_club(&r.name),
                culture = culture_name(&r.culture),
                continent = continent_name(r.continent)
            )
        }),
    };
    tr!("dated", date = date_text(news.week, data), what = what)
}

/// "Arrivé récemment : vendable à partir de la saison 1, semaine 16" for a fighter who
/// joined its club less than half a season ago, `None` for one that may be sold.
pub fn unsellable_text(
    fighter: &CareerFighter,
    state: &GameState,
    data: &DesignData,
) -> Option<String> {
    (!state.may_be_sold(fighter, data)).then(|| {
        let from = date_in_sentence(GameState::sellable_from(fighter, data), data);
        tr!("unsellable", date = from)
    })
}

pub fn standing_name(standing: Standing) -> String {
    match standing {
        Standing::Ally => tr!("standing-ally"),
        Standing::Friend => tr!("standing-friend"),
        Standing::Neutral => tr!("standing-neutral"),
        Standing::Tense => tr!("standing-tense"),
        Standing::Rival => tr!("standing-rival"),
    }
}

/// "+12, ami": what rival club `club` thinks of the player's club.
pub fn relation_text(club: usize, state: &GameState, data: &DesignData) -> String {
    format!(
        "{:+}, {}",
        state.relation(club),
        standing_name(state.standing(club, data))
    )
}

/// "-30 : Augure n° 1 a tué Guerrier n° 3 (saison 1, semaine 5)": one change of a
/// rival club's relation, with its reason.
pub fn relation_change_text(change: Change, state: &GameState, data: &DesignData) -> String {
    let club = |i| club_fighter(i, state, data);
    let rival = |id| rival_fighter_name(id, state, data);
    let why = match change.why {
        Reason::Killed { fighter, victim } => {
            tr!(
                "reason-killed",
                fighter = club(fighter),
                rival = rival(victim)
            )
        }
        Reason::Beaten { fighter, rival: r } => {
            tr!("reason-beaten", fighter = club(fighter), rival = rival(r))
        }
        Reason::TitleTaken { fighter, colosseum } => tr!(
            "reason-title-taken",
            fighter = club(fighter),
            place = data
                .world
                .colosseums
                .get(colosseum)
                .map_or_else(|| "?".into(), colosseum_rules_name)
        ),
        Reason::ChallengeRefused { fighter, rival: r } => {
            let rival = rival(r);
            tr!(
                "reason-challenge-refused",
                fighter = club(fighter),
                vowel = vowel(&rival),
                rival = rival
            )
        }
        Reason::Sold { fighter } => tr!("reason-sold", fighter = club(fighter)),
        Reason::Bought { fighter } => tr!("reason-bought", fighter = club(fighter)),
        Reason::PactKept { fighter, rival: r } => {
            tr!(
                "reason-pact-kept",
                fighter = club(fighter),
                rival = rival(r)
            )
        }
        Reason::Betrayed { fighter, rival: r } => {
            tr!("reason-betrayed", fighter = club(fighter), rival = rival(r))
        }
    };
    tr!(
        "relation-change",
        points = format!("{:+}", change.points),
        why = why,
        date = date_in_sentence(change.week, data)
    )
}

/// "Saison 2, semaine 7".
pub fn date_text(week: u64, data: &DesignData) -> String {
    let when = Date::of(week, &data.calendar);
    tr!("date", season = when.season, week = when.week)
}

/// "saison 2, semaine 7", within a sentence.
pub fn date_in_sentence(week: u64, data: &DesignData) -> String {
    let when = Date::of(week, &data.calendar);
    tr!("date-in-sentence", season = when.season, week = when.week)
}

pub fn colosseum_name(id: ColosseumId, data: &DesignData) -> String {
    data.world
        .colosseum(id)
        .map_or_else(|| "?".into(), colosseum_rules_name)
}

pub fn colosseum_rules_name(rules: &ColosseumData) -> String {
    data_name("colosseum", &rules.key)
}

/// The shown name of a design data entry: `<kind>-<key>` in the language files.
fn data_name(kind: &str, key: &str) -> String {
    tr!(&format!("{kind}-{key}"))
}

pub fn profile_name(profile: Profile) -> String {
    match profile {
        Profile::Cautious => tr!("profile-cautious"),
        Profile::Aggressive => tr!("profile-aggressive"),
        Profile::Merchant => tr!("profile-merchant"),
        Profile::Mentor => tr!("profile-mentor"),
        Profile::Veteran => tr!("profile-veteran"),
        Profile::Showman => tr!("profile-showman"),
    }
}

/// "pauvre", "modeste" or "riche".
pub fn wealth_name(wealth: Wealth) -> String {
    match wealth {
        Wealth::Poor => tr!("wealth-poor"),
        Wealth::Modest => tr!("wealth-modest"),
        Wealth::Rich => tr!("wealth-rich"),
    }
}

/// "Europe", "Asie", "Amériques", "Afrique" or "Océanie".
pub fn continent_name(continent: Continent) -> String {
    match continent {
        Continent::Europe => tr!("continent-europe"),
        Continent::Asia => tr!("continent-asia"),
        Continent::Americas => tr!("continent-americas"),
        Continent::Africa => tr!("continent-africa"),
        Continent::Oceania => tr!("continent-oceania"),
    }
}

/// "Combat à Rome (60)": `activity` with its entry `fee`, if any.
pub fn priced(activity: Activity, fee: u32, data: &DesignData) -> String {
    let name = activity_name(activity, data);
    if fee > 0 {
        format!("{name} ({fee})")
    } else {
        name
    }
}

/// "Club modeste": the club's wealth.
pub fn wealth_text(wealth: Wealth) -> String {
    tr!("wealth", wealth = wealth_name(wealth))
}

/// The name of rival club `club`.
pub fn club_name(club: usize, state: &GameState) -> String {
    state
        .rivals
        .get(club)
        .map_or_else(|| "?".into(), |r| named_club(&r.name))
}

/// "Palestre d'Achille", "Ludus Magnus II": its pattern filled with its name, told
/// whether that name starts with a vowel for the languages that elide before one.
pub fn named_club(name: &ClubName) -> String {
    let club = tr!(
        &format!("club-{}", name.pattern),
        name = name.name.as_str(),
        vowel = vowel(&name.name)
    );
    if name.number < 2 {
        club
    } else {
        tr!("club-numbered", club = club, number = roman(name.number))
    }
}

fn is_vowel(c: char) -> bool {
    "AEIOUYÂÄÀÉÈÊËÎÏÔÖÛÜŪĀĒĪŌ".contains(c)
}

/// "true" when `name` starts with a vowel, for the `$vowel` of the languages that
/// elide before one ("le défi d'Ursus").
pub fn vowel(name: &str) -> &'static str {
    if name.starts_with(is_vowel) {
        "true"
    } else {
        "false"
    }
}

/// `n` in Roman numerals, from 1 to 39 (beyond, a figure).
fn roman(n: u32) -> String {
    if n >= 40 {
        return n.to_string();
    }
    let units = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"];
    let tens = "X".repeat(usize::try_from(n / 10).unwrap_or(0));
    format!("{tens}{}", units[usize::try_from(n % 10).unwrap_or(0)])
}

/// The name of culture `key` ("Asie de l'Est").
pub fn culture_name(key: &str) -> String {
    data_name("culture", key)
}

/// "culture : Asie de l'Est".
pub fn culture_text(key: &str) -> String {
    tr!("culture", culture = culture_name(key))
}

pub fn arena_name(arena: ArenaKind) -> String {
    match arena {
        ArenaKind::Sand => tr!("arena-sand"),
        ArenaKind::Pillars => tr!("arena-pillars"),
        ArenaKind::Narrow => tr!("arena-narrow"),
    }
}

/// What an arena kind does to the fight, for the player.
pub fn arena_effect(arena: ArenaKind) -> String {
    match arena {
        ArenaKind::Sand => tr!("arena-sand-effect"),
        ArenaKind::Pillars => tr!("arena-pillars-effect"),
        ArenaKind::Narrow => tr!("arena-narrow-effect"),
    }
}

/// How a colosseum's ranking names `contender`; the club's fighters are marked.
pub fn contender_name(contender: Contender, state: &GameState, data: &DesignData) -> String {
    match contender {
        Contender::Club(i) => tr!(
            "contender-yours",
            fighter = club_fighter(i, state, data),
            club = named_club(&state.name)
        ),
        Contender::Rival(id) => rival_fighter_name(id, state, data),
        Contender::FreeAgent(a) => target_name(Target::FreeAgent(a), state, data),
    }
}

/// The key a side is selected by in the language files.
fn side_key(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}

pub fn location_name(location: Location, dominant: Side) -> String {
    match location {
        Location::Arm(s) if s == dominant => tr!("location-arm-dominant", side = side_key(s)),
        _ => part_name(location),
    }
}

/// A body part's name, without saying which arm is dominant.
pub fn part_name(location: Location) -> String {
    match location {
        Location::Head => tr!("location-head"),
        Location::Body => tr!("location-body"),
        Location::Arm(s) => tr!("location-arm", side = side_key(s)),
        Location::Leg(s) => tr!("location-leg", side = side_key(s)),
    }
}

pub fn weeks(n: u32) -> String {
    tr!("weeks", n = n)
}

pub fn injury_text(injury: Injury, dominant: Side) -> String {
    injury_at(injury, location_name(injury.location, dominant))
}

/// An injury, without saying whether the arm is the dominant one.
pub fn part_injury_text(injury: Injury) -> String {
    injury_at(injury, part_name(injury.location))
}

/// An injury, its location already named.
fn injury_at(injury: Injury, location: String) -> String {
    let severity = match injury.severity {
        Severity::Light => "light",
        Severity::Medium => "medium",
        Severity::Severe => "severe",
        Severity::Permanent => return tr!("injury-permanent", location = location),
        Severity::Lost => {
            let leg = matches!(injury.location, Location::Leg(_));
            return tr!(
                "injury-lost",
                location = location,
                leg = if leg { "true" } else { "false" }
            );
        }
    };
    tr!(
        "injury",
        location = location,
        severity = severity,
        weeks = weeks(injury.weeks_left)
    )
}

pub fn illness_text(illness: Illness) -> String {
    let level = match illness.level {
        IllnessLevel::Mild => "mild",
        IllnessLevel::Moderate => "moderate",
        IllnessLevel::Severe => "severe",
    };
    tr!("illness", level = level, weeks = weeks(illness.weeks_left))
}

/// A severed limb in a sentence: "bras gauche tranché", "jambe droite tranchée".
fn limb_cut(location: Location) -> String {
    match location {
        Location::Head => tr!("cut-head"),
        Location::Body => tr!("cut-body"),
        Location::Arm(s) => tr!("cut-arm", side = side_key(s)),
        Location::Leg(s) => tr!("cut-leg", side = side_key(s)),
    }
}

pub fn fight_text(fight: &FightLine, data: &DesignData) -> String {
    let mine = fight.mine.as_ref();
    // Brought to 0 HP, it may still have survived the fatal blow.
    let fell = mine.is_some_and(|f| f.hp == 0);
    let knocked_out = mine.is_some_and(|f| f.knocked_out);
    let severed = mine.and_then(|f| f.severed);
    let result = match (fight.result, fight.died, severed) {
        (_, true, Some(Location::Head)) => tr!("result-beheaded"),
        (_, true, _) => tr!("result-dead"),
        (FightResult::Lost, false, Some(limb)) => tr!("result-severed", limb = limb_cut(limb)),
        (FightResult::Lost, false, _) if fell => tr!("result-left-for-dead"),
        (FightResult::Lost, false, _) if knocked_out => tr!("result-knocked-out"),
        (FightResult::Won, ..) => tr!("result-won"),
        (FightResult::Lost, ..) => tr!("result-lost"),
        (FightResult::GaveUp, ..) => tr!("result-gave-up"),
        (FightResult::Unfinished, ..) => tr!(
            "result-unfinished",
            minutes = fight.max_ticks / (lanista_core::combat::TICKS_PER_SECOND * 60)
        ),
    };
    let stronger = if fight.against_stronger {
        "true"
    } else {
        "false"
    };
    let refused = if fight.refused.armour {
        "armour"
    } else {
        "none"
    };
    let against = if fight.is_melee() {
        tr!(
            "against-melee",
            size = fight.size,
            place = place_name(fight.place),
            stronger = stronger
        )
    } else {
        let class = fight
            .opponent
            .and_then(|class| data.class(class))
            .map_or_else(|| "?".into(), |c| data_name("class", &c.key));
        tr!("against-duel", class = class, stronger = stronger)
    };
    tr!(
        "fight",
        against = against,
        refused = refused,
        result = result,
        fame = format!("{:+}", fight.fame_change)
    )
}

/// A place in a melee: « 1er », « 2e »…
pub fn place_name(place: u32) -> String {
    tr!("place", place = place)
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
    let money = tr!("money");
    let line = if before == after {
        format!("{money} {after}")
    } else {
        format!("{money} {after} ({:+})", after - before)
    };
    let moves: Vec<String> = ledger
        .iter()
        .map(|t| {
            let name = match t.flow {
                Flow::Upkeep => tr!("flow-upkeep"),
                Flow::Care => tr!("flow-care"),
                Flow::Training => tr!("flow-training"),
                Flow::EntryFee => tr!("flow-entry-fee"),
                Flow::WinPrize => tr!("flow-win-prize"),
                Flow::Receipts => tr!("flow-receipts"),
                Flow::ChampionBonus => tr!("flow-champion-bonus"),
                Flow::Purchase => tr!("flow-purchase"),
                Flow::Sale => tr!("flow-sale"),
                Flow::Salary => tr!("flow-salary"),
            };
            format!("{name} {:+}", t.amount)
        })
        .collect();
    if moves.is_empty() {
        line
    } else {
        tr!("detailed", what = line, details = moves.join(", "))
    }
}

/// What a week changed for a fighter whose dominant hand is `dominant`, in the
/// player's language.
pub fn week_lines(week: &FighterWeek, dominant: Side, data: &DesignData) -> Vec<String> {
    let mut lines = vec![activity_name(week.activity, data)];
    if let Some(fight) = &week.fight {
        lines.push(fight_text(fight, data));
    }
    if let Some(injury) = &week.new_injury {
        lines.push(tr!("week-injury", injury = injury_text(*injury, dominant)));
    }
    if week.after_effect_spared {
        lines.push(tr!("week-after-effect-spared"));
    }
    for injury in &week.healed_injuries {
        lines.push(tr!(
            "week-healed",
            location = location_name(injury.location, dominant)
        ));
    }
    if let Some(illness) = &week.new_illness {
        lines.push(tr!("week-ill", illness = illness_text(*illness)));
    }
    if week.illness_healed {
        lines.push(tr!("week-illness-healed"));
    }
    if week.birthday {
        lines.push(tr!("week-birthday", age = week.age));
        if week.season_gains > 0 {
            lines.push(tr!("week-season-gains", gains = week.season_gains));
        }
    }
    lines.push(format!(
        "{}, {}, {}",
        change(&tr!("form"), week.form.0, week.form.1),
        change(&tr!("morale"), week.morale.0, week.morale.1),
        change(&tr!("fame"), week.fame.0, week.fame.1)
    ));
    let (before, after) = week.stats;
    let stats: Vec<String> = stat_names()
        .iter()
        .zip(before.to_array().into_iter().zip(after.to_array()))
        .filter(|(_, (b, a))| a != b)
        .map(|(name, (b, a))| format!("{name} {:+}", i64::from(a) - i64::from(b)))
        .collect();
    if !stats.is_empty() {
        lines.push(tr!("week-stats", stats = stats.join(", ")));
    }
    match week.retired {
        None => {}
        Some(Retired::Maimed) => lines.push(tr!("week-retires-maimed")),
        Some(Retired::OfAge) => lines.push(tr!("week-retires", age = week.age)),
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_melee_is_told_by_its_size_and_place() {
        use lanista_core::combat::ArenaKind;
        use lanista_core::fighter::melee::play_melee;
        use rand_chacha::ChaCha8Rng;
        use rand_chacha::rand_core::SeedableRng;

        let data = DesignData::load().unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut fighters: Vec<CareerFighter> = (0..5)
            .map(|_| CareerFighter::random(&mut rng, &data))
            .collect();
        let melee = play_melee(&mut fighters, &[], ArenaKind::Sand, &mut rng, &data);
        let summary = melee.summary(2).unwrap();
        let text = fight_text(&FightLine::new(&summary, &data), &data);
        let place = place_name(melee.places[2]);
        assert!(text.starts_with(&format!("Mêlée à 5, {place}")), "{text}");
        assert_eq!(place_name(1), "1er");
        assert_eq!(place_name(3), "3e");
    }

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
        let lost = |location| Injury {
            location,
            severity: Severity::Lost,
            weeks_left: 0,
        };
        assert_eq!(
            injury_text(lost(Location::Arm(Side::Left)), Side::Left),
            "Bras gauche (dominant) : perdu"
        );
        assert_eq!(
            injury_text(lost(Location::Leg(Side::Right)), Side::Left),
            "Jambe droite : perdue"
        );
        assert_eq!(
            location_name(Location::Arm(Side::Right), Side::Left),
            "Bras droit"
        );
    }
}
