//! What the player reads, from the language files: fighters, injuries, illnesses, fights,
//! weeks, money, colosseums, the market and the staff.

use lanista_core::calendar::{Date, roman_numeral};
use lanista_core::colosseum::{
    ColosseumData, ColosseumId, Contender, Continent, TitleNews, title_kept,
};
use lanista_core::combat::ArenaKind;
use lanista_core::design_data::DesignData;
use lanista_core::economy::{Flow, Terms, Transaction, Wealth};
use lanista_core::fighter::STAT_COUNT;
use lanista_core::fighter::career::{Activity, CareerFighter, Record, Status};
use lanista_core::fighter::fame::FightResult;
use lanista_core::fighter::illness::{Illness, IllnessLevel};
use lanista_core::fighter::injury::{Injury, Location, Severity, Side};
use lanista_core::fighter::personality::Temperament;
use lanista_core::fighter::traits::Trait;
use lanista_core::journal::{FightLine, FighterWeek, Retired};
use lanista_core::market::{
    MarketNews, Move, Target, ease_seen, personality_seen, potential_range, stat_ranges,
};
use lanista_core::names::{ClubName, FighterName};
use lanista_core::relations::{Change, Reason, Standing};
use lanista_core::rivals::{Profile, RivalId};
use lanista_core::staff::{Candidate, Role, Scouting, Staff, StaffMove, StaffNews};
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
    origin_place(fighter, data).map(|place| tr!("sheet-origin", place = place))
}

/// "Grèce (Europe)": [`origin_text`] without its label.
pub fn origin_place(fighter: &CareerFighter, data: &DesignData) -> Option<String> {
    let culture = data.culture(&fighter.culture)?;
    Some(tr!(
        "origin-place",
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

fn temperament_key(temperament: Temperament) -> &'static str {
    match temperament {
        Temperament::Balanced => "balanced",
        Temperament::Reckless => "reckless",
        Temperament::Showman => "showman",
        Temperament::Methodical => "methodical",
        Temperament::Survivor => "survivor",
        Temperament::Berserker => "berserker",
        Temperament::Proud => "proud",
    }
}

/// A temperament's name: "Téméraire".
pub fn temperament_name(temperament: Temperament) -> String {
    tr!(&format!("temperament-{}", temperament_key(temperament)))
}

/// What a temperament does, in a sentence or two.
pub fn temperament_text(temperament: Temperament) -> String {
    tr!(&format!(
        "temperament-{}-text",
        temperament_key(temperament)
    ))
}

fn trait_key(t: Trait) -> &'static str {
    match t {
        Trait::Workaholic => "workaholic",
        Trait::Nonchalant => "nonchalant",
        Trait::Vain => "vain",
        Trait::Spiteful => "spiteful",
        Trait::Superstitious => "superstitious",
        Trait::PackLeader => "pack-leader",
    }
}

/// A trait's name: "vaniteux".
pub fn trait_name(t: Trait) -> String {
    tr!(&format!("trait-{}", trait_key(t)))
}

/// What a trait does, in a sentence or two.
pub fn trait_text(t: Trait) -> String {
    tr!(&format!("trait-{}-text", trait_key(t)))
}

/// What a trait of `fighter` remembers: "Rancune contre Flamma", "Sol fétiche : sable".
pub fn trait_memory(fighter: &CareerFighter, t: Trait) -> Option<String> {
    match t {
        Trait::Spiteful if !fighter.grudges.is_empty() => {
            let names: Vec<String> = fighter.grudges.iter().map(|(_, n)| stage_name(n)).collect();
            Some(tr!("trait-grudges", names = names.join(", ")))
        }
        Trait::Superstitious => fighter
            .lucky_floor
            .map(|floor| tr!("trait-lucky-floor", floor = arena_name(floor))),
        _ => None,
    }
}

/// Its temperament and traits: "Téméraire, vaniteux".
pub fn personality_text(fighter: &CareerFighter) -> String {
    let temperament = temperament_name(fighter.personality.temperament);
    let traits = fighter.traits.iter().map(|&t| trait_name(t));
    std::iter::once(temperament)
        .chain(traits)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The temperament and traits of a fighter out of the club, if the recruiter sees
/// them, or "?".
pub fn outside_personality_text(fighter: &CareerFighter, scouting: &Scouting) -> String {
    if !personality_seen(fighter, scouting) {
        return "?".into();
    }
    personality_text(fighter)
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

/// "8 / sem., prime de victoire 5, 2 saisons de plus".
pub fn terms_text(terms: Terms) -> String {
    tr!(
        "terms",
        wage = terms.wage,
        bonus = terms.win_bonus,
        seasons = terms.seasons
    )
}

/// A market move chosen for the week, in one line.
pub fn move_text(m: Move, state: &GameState, data: &DesignData) -> String {
    let club = |i| club_fighter(i, state, data);
    match m {
        Move::Observe(target) => tr!("move-observe", fighter = target_name(target, state, data)),
        Move::Sign(a, terms) => tr!(
            "move-sign",
            fighter = target_name(Target::FreeAgent(a), state, data),
            terms = terms_text(terms)
        ),
        Move::Release(i) => tr!("move-release", fighter = club(i)),
        Move::List(i) => tr!("move-list", fighter = club(i)),
        Move::Unlist(i) => tr!("move-unlist", fighter = club(i)),
        Move::Keep(i) => tr!("move-keep", fighter = club(i)),
        Move::Unkeep(i) => tr!("move-unkeep", fighter = club(i)),
        Move::Renew(i, terms) => tr!("move-renew", fighter = club(i), terms = terms_text(terms)),
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
        MarketNews::Renewed(i) => tr!("market-renewed", fighter = club(i)),
        MarketNews::RenewalRefused(i) => tr!("market-renewal-refused", fighter = club(i)),
        MarketNews::TermsRefused(i) => tr!("market-terms-refused", fighter = club(i)),
        MarketNews::SignRefused(a) => tr!(
            "market-sign-refused",
            fighter = target_name(Target::FreeAgent(a), state, data)
        ),
        MarketNews::ContractEnded(i) => tr!("market-contract-ended", fighter = club(i)),
    }
}

/// "Augure n° 3 (Ludus Magnus) à Capoue": who would receive the challenge, and where.
pub fn challenger_text(
    colosseum: ColosseumId,
    rival: RivalId,
    state: &GameState,
    data: &DesignData,
) -> String {
    tr!(
        "challenger",
        fighter = rival_fighter_name(rival, state, data),
        place = colosseum_name(colosseum, data)
    )
}

/// A piece of the club's newspaper: a headline, who it is about in small print, and
/// whether it is about the player's club.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    pub headline: String,
    pub byline: Option<String>,
    pub ours: bool,
}

/// How many ways a headline may be told; which one is picked by the week and the
/// colosseum, so that a page does not repeat itself and stays the same when reloaded.
const HEADLINE_VARIANTS: u64 = 3;

/// "Flamma", "Augure n° 3": `contender` by its name alone, for a headline.
fn contender_short_name(contender: Contender, state: &GameState, data: &DesignData) -> String {
    let (fighter, number) = match contender {
        Contender::Club(i) => (state.fighters.get(i), i),
        Contender::Rival(id) => (state.rival_fighter(id), id.fighter),
        Contender::FreeAgent(a) => (state.free_agents.get(a), a),
    };
    match fighter {
        Some(f) if !f.name.is_empty() => stage_name(&f.name),
        Some(f) => tr!(
            "fighter-unnamed",
            class = class_name(f, data),
            number = number + 1
        ),
        None => "?".into(),
    }
}

/// What happened in week `week` to the title of the colosseum at index `c`, as the
/// newspaper tells it.
pub fn title_article(
    week: u64,
    c: usize,
    news: TitleNews,
    state: &GameState,
    data: &DesignData,
) -> Article {
    let Some(rules) = data.world.colosseums.get(c) else {
        return Article {
            headline: String::new(),
            byline: None,
            ours: false,
        };
    };
    let short = |contender| contender_short_name(contender, state, data);
    let full = |contender| contender_name(contender, state, data);
    let ours = |contender: Option<Contender>| matches!(contender, Some(Contender::Club(_)));
    let place = colosseum_rules_name(rules);
    let variant = (week + u64::try_from(c).unwrap_or(0)) % HEADLINE_VARIANTS;
    let heir = |key: &str, holder: Option<Contender>| {
        holder.map_or_else(
            || tr!(&format!("{key}-empty"), place = place.as_str()),
            |holder| tr!(key, place = place.as_str(), holder = short(holder)),
        )
    };
    let heir_byline =
        |holder: Option<Contender>| holder.map(|holder| tr!("byline-heir", holder = full(holder)));
    match news {
        TitleNews::Fight {
            champion,
            challenger,
            result,
        } => {
            let key = match result {
                FightResult::Unfinished => "headline-title-draw",
                _ if title_kept(result) => "headline-title-kept",
                _ => "headline-title-taken",
            };
            Article {
                headline: tr!(
                    key,
                    variant = variant,
                    place = place.as_str(),
                    champion = short(champion),
                    challenger = short(challenger)
                ),
                byline: Some(tr!(
                    "byline-versus",
                    champion = full(champion),
                    challenger = full(challenger)
                )),
                ours: ours(Some(champion)) || ours(Some(challenger)),
            }
        }
        TitleNews::Stripped { former, to } => Article {
            headline: tr!(
                "headline-title-stripped",
                place = place.as_str(),
                former = short(former),
                heir = heir("headline-heir", to)
            ),
            byline: heir_byline(to),
            ours: ours(Some(former)) || ours(to),
        },
        TitleNews::Vacated { to } => Article {
            headline: heir("headline-title-vacated", to),
            byline: heir_byline(to),
            ours: ours(to),
        },
    }
}

/// Whether a week's title news is a title changing hands rather than one defended.
pub fn title_changed(news: TitleNews) -> bool {
    match news {
        TitleNews::Fight { result, .. } => !title_kept(result),
        TitleNews::Stripped { .. } | TitleNews::Vacated { .. } => true,
    }
}

/// A world news item, as the newspaper tells it: "Faillite : Ludus Magnus ferme ses
/// portes, ses combattants partent au marché".
pub fn world_article(news: News, state: &GameState, data: &DesignData) -> Article {
    let article = |headline| Article {
        headline,
        byline: None,
        ours: false,
    };
    match news.what {
        WorldNews::Title(c, title) => title_article(news.week, c, title, state, data),
        WorldNews::ChampionBought {
            colosseum,
            fighter,
            buyer,
            price,
        } => Article {
            headline: tr!(
                "headline-champion-bought",
                club = club_name(buyer, state),
                fighter = contender_short_name(Contender::Rival(fighter), state, data),
                place = data
                    .world
                    .colosseums
                    .get(colosseum)
                    .map_or_else(|| "?".into(), colosseum_rules_name),
                price = price
            ),
            byline: Some(rival_fighter_name(fighter, state, data)),
            ours: false,
        },
        WorldNews::Closed(club) => article(tr!("headline-closed", club = club_name(club, state))),
        WorldNews::Rivalry(club, declared) => Article {
            headline: tr!(
                if declared {
                    "headline-rivalry"
                } else {
                    "headline-rivalry-ended"
                },
                club = club_name(club, state)
            ),
            byline: None,
            ours: true,
        },
        WorldNews::Founded(club) => article(state.rivals.get(club).map_or_else(String::new, |r| {
            tr!(
                "headline-founded",
                club = named_club(&r.name),
                culture = culture_name(&r.culture),
                continent = continent_name(r.continent)
            )
        })),
    }
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

/// "Anno DCLXXX ab Urbe condita": the year of the season of `week`, since the
/// founding of Rome, in Roman numerals.
pub fn year_text(week: u64, data: &DesignData) -> String {
    let year = Date::of(week, &data.calendar).year(&data.calendar);
    tr!("year", year = roman_numeral(year))
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
    tr!(
        "fight",
        against = against_text(fight, data),
        result = result_text(fight),
        fame = format!("{:+}", fight.fame_change)
    )
}

/// How the fight ended for the club's fighter: « victoire », « abandon », « décapité »…
pub fn result_text(fight: &FightLine) -> String {
    let mine = fight.mine.as_ref();
    // Brought to 0 HP, it may still have survived the fatal blow.
    let fell = mine.is_some_and(|f| f.hp == 0);
    let knocked_out = mine.is_some_and(|f| f.knocked_out);
    let severed = mine.and_then(|f| f.severed);
    match (fight.result, fight.died, severed) {
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
    }
}

/// Who the club's fighter faced: « Contre un Augure, plus fort, sans armure ».
pub fn against_text(fight: &FightLine, data: &DesignData) -> String {
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
    tr!("fight-against", against = against, refused = refused)
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
    // One sum per flow, in the order they first came: three fighters' upkeep is one
    // « entretien -30 ».
    let mut sums: Vec<(Flow, i64)> = Vec::new();
    for t in ledger {
        match sums.iter_mut().find(|(flow, _)| *flow == t.flow) {
            Some((_, sum)) => *sum += t.amount,
            None => sums.push((t.flow, t.amount)),
        }
    }
    let moves: Vec<String> = sums
        .into_iter()
        .map(|(flow, amount)| {
            let name = match flow {
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
                Flow::Wage => tr!("flow-wage"),
                Flow::WinBonus => tr!("flow-win-bonus"),
            };
            format!("{name} {amount:+}")
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
    for &t in &week.new_traits {
        lines.push(tr!("week-trait", name = trait_name(t)));
    }
    for name in &week.revenges {
        lines.push(tr!("week-revenge", name = stage_name(name)));
    }
    for name in &week.new_grudges {
        lines.push(tr!("week-grudge", name = stage_name(name)));
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
    fn a_week_tells_grudges_kept_and_revenges_taken() {
        use lanista_core::fighter::career::WeekPlan;
        use lanista_core::fighter::equipment::Refused;
        use lanista_core::staff::StaffEffects;
        use rand_chacha::ChaCha8Rng;
        use rand_chacha::rand_core::SeedableRng;

        let data = DesignData::load().unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let name = |name: &str| FighterName {
            name: name.into(),
            number: 0,
        };
        let mut before = CareerFighter::random(&mut rng, &data);
        before.grudges = vec![(2, name("Flamma")), (4, name("Spiculus"))];
        let mut after = before.clone();
        let rest = WeekPlan {
            activity: Activity::Rest,
            care: false,
            refused: Refused::NONE,
            loadout: 0,
        };
        let report = after
            .play_week(rest, None, StaffEffects::default(), &mut rng, &data)
            .unwrap();
        after.grudges = vec![(4, name("Spiculus")), (3, name("Crixus"))];
        let week = FighterWeek::new(1, (&before, &after), &report, &data);
        let lines = week_lines(&week, Side::Right, &data);
        assert!(
            lines.contains(&"Prend sa revanche sur Flamma".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"Garde rancune à Crixus".to_owned()),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("Spiculus")), "{lines:?}");
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
