//! What the player reads, in French: fighters, injuries, illnesses, fights,
//! weeks, money, colosseums, the market and the staff.

use lanista_core::calendar::Date;
use lanista_core::colosseum::{
    Challenge, ColosseumId, Contender, Continent, TitleNews, title_kept,
};
use lanista_core::combat::ArenaKind;
use lanista_core::design_data::DesignData;
use lanista_core::economy::{Flow, Transaction, Wealth};
use lanista_core::fighter::STAT_COUNT;
use lanista_core::fighter::care::CareRules;
use lanista_core::fighter::career::{
    Activity, CareerFighter, FightSummary, Record, Status, WeekReport,
};
use lanista_core::fighter::fame::FightResult;
use lanista_core::fighter::illness::{Illness, IllnessLevel};
use lanista_core::fighter::injury::{Injury, Location, Severity, Side};
use lanista_core::fighter::personality::Personality;
use lanista_core::fighter::retirement::too_maimed;
use lanista_core::market::{
    MarketNews, Move, Target, personality_seen, potential_range, stat_ranges,
};
use lanista_core::relations::{Change, Reason, Standing};
use lanista_core::rivals::{Profile, RivalId};
use lanista_core::staff::{
    Candidate, DoctorRules, RecruiterRules, Role, Scouting, Staff, StaffMove, StaffNews,
    TrainerRules,
};
use lanista_core::state::{GameState, News, WorldNews};

/// Stat names, in `Stats::to_array` order.
pub const STAT_NAMES: [&str; STAT_COUNT] = ["PV", "AC", "AL", "PRÉ", "VIT", "DC", "DL"];

/// How the game names the fighter at `index` in the club, until fighters have names.
pub fn fighter_name(index: usize, fighter: &CareerFighter, data: &DesignData) -> String {
    format!("{} n° {}", class_name(fighter, data), index + 1)
}

/// One word for a fighter's state, worst first.
pub fn condition_text(fighter: &CareerFighter) -> &'static str {
    match fighter.status {
        Status::Dead => "mort",
        Status::Retired => "retraité",
        Status::Sold => "vendu",
        Status::Released => "libéré",
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
        Activity::TitleFight(id) => format!("Combat pour le titre à {}", colosseum_name(id, data)),
        Activity::Challenge(id, _) => format!("Défi à {}", colosseum_name(id, data)),
        Activity::Melee(id) => format!("Mêlée à {}", colosseum_name(id, data)),
    }
}

/// How the market names the free agent at `index`.
pub fn free_agent_name(index: usize, fighter: &CareerFighter, data: &DesignData) -> String {
    format!(
        "Free agent n° {} ({})",
        index + 1,
        class_name(fighter, data)
    )
}

/// "Augure n° 3 (Ludus Magnus)": how the game names a rival club's fighter.
pub fn rival_fighter_name(id: RivalId, state: &GameState, data: &DesignData) -> String {
    let class = state
        .rival_fighter(id)
        .map_or_else(|| "?".into(), |f| class_name(f, data));
    format!(
        "{class} n° {} ({})",
        id.fighter + 1,
        club_name(id.club, state)
    )
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
    format!(
        "Agressivité {}, prudence {}, goût du spectacle {}, détermination {}",
        p.aggression.value(),
        p.caution.value(),
        p.showmanship.value(),
        p.determination.value()
    )
}

/// The personality of a fighter out of the club, in short, if the recruiter sees it:
/// "Agr 60, Pru 40, Spe 50, Dét 70", or "?".
pub fn outside_personality_text(fighter: &CareerFighter, scouting: &Scouting) -> String {
    if !personality_seen(fighter, scouting) {
        return "?".into();
    }
    let p = &fighter.personality;
    format!(
        "Agr {}, Pru {}, Spe {}, Dét {}",
        p.aggression.value(),
        p.caution.value(),
        p.showmanship.value(),
        p.determination.value()
    )
}

/// "3 V, 1 D, 2 N".
pub fn record_text(record: Record) -> String {
    format!("{} V, {} D, {} N", record.wins, record.losses, record.draws)
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
        Move::Observe(target) => format!("Observer {}", target_name(target, state, data)),
        Move::Sign(a) => format!(
            "Recruter {}",
            target_name(Target::FreeAgent(a), state, data)
        ),
        Move::Release(i) => format!("Libérer {}", club(i)),
        Move::List(i) => format!("Mettre en vente {}", club(i)),
        Move::Unlist(i) => format!("Retirer {} de la vente", club(i)),
        Move::Keep(i) => format!("Rendre {} invendable", club(i)),
        Move::Unkeep(i) => format!("Rendre {} vendable", club(i)),
        Move::AcceptOffer { fighter, rival } => {
            let price = state
                .sale_offers
                .iter()
                .find(|o| o.fighter == fighter && o.rival == rival)
                .map_or(0, |o| o.price);
            format!(
                "Vendre {} à {} pour {price}",
                club(fighter),
                club_name(rival, state)
            )
        }
        Move::Offer(id, price) => format!(
            "Offrir {price} pour {}",
            rival_fighter_name(id, state, data)
        ),
    }
}

/// The name of a post, as in "un médecin".
pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Trainer => "entraîneur",
        Role::Doctor => "médecin",
        Role::Recruiter => "recruteur",
    }
}

/// The name of a post, as a title.
pub fn role_title(role: Role) -> &'static str {
    match role {
        Role::Trainer => "Entraîneur",
        Role::Doctor => "Médecin",
        Role::Recruiter => "Recruteur",
    }
}

/// A club's staff: "Personnel : entraîneur de niveau 5, médecin de niveau 2", or
/// "Personnel : aucun".
pub fn staff_text(staff: &Staff) -> String {
    let posts: Vec<String> = Role::ALL
        .into_iter()
        .filter_map(|role| {
            let level = staff.level(role)?;
            Some(format!("{} de niveau {level}", role_name(role)))
        })
        .collect();
    if posts.is_empty() {
        "Personnel : aucun".into()
    } else {
        format!("Personnel : {}", posts.join(", "))
    }
}

/// A change of staff, as chosen for the week among the candidates on `market`.
pub fn staff_move_text(m: StaffMove, market: &[Candidate]) -> String {
    match m {
        StaffMove::Hire(c) => match market.get(c) {
            Some(candidate) => {
                let person = &candidate.person;
                format!(
                    "Engager un {} de niveau {}",
                    role_name(person.role()),
                    person.level()
                )
            }
            None => "Engager un candidat parti".into(),
        },
        StaffMove::Dismiss(Role::Trainer) => "Renvoyer l'entraîneur".into(),
        StaffMove::Dismiss(role) => format!("Renvoyer le {}", role_name(role)),
    }
}

/// A change of staff, once made.
pub fn staff_news_text(news: StaffNews) -> String {
    match news {
        StaffNews::Hired(role, level) => format!("{} de niveau {level} engagé", role_title(role)),
        StaffNews::Dismissed(role) => format!("{} renvoyé", role_title(role)),
    }
}

/// How long a candidate stays on the market: "encore 3 semaines", or "dernière semaine".
pub fn weeks_left_text(weeks: u32) -> String {
    if weeks <= 1 {
        "dernière semaine".into()
    } else {
        format!("encore {weeks} semaines")
    }
}

/// What a trainer costs and brings: "salaire 20 par semaine, séance 6, progression
/// +20 %, 1 de forme épargnée par séance".
pub fn trainer_text(trainer: &TrainerRules) -> String {
    format!(
        "salaire {} par semaine, séance {}, progression +{} %, {} de forme épargnée par séance",
        trainer.salary,
        trainer.training_cost,
        trainer.coaching.progress_percent,
        trainer.coaching.form_spared
    )
}

/// What a doctor costs and brings: "salaire 20 par semaine, les soins laissent 40 %
/// des semaines (50 sans médecin), prix des soins 110 %, séquelle évitée 30 fois sur 100".
pub fn doctor_text(doctor: &DoctorRules, care: &CareRules) -> String {
    let medicine = &doctor.medicine;
    let kept = medicine.care_rules(care).weeks_kept_percent;
    format!(
        "salaire {} par semaine, les soins laissent {kept} % des semaines ({} sans médecin), \
         prix des soins {} %, séquelle évitée {} fois sur 100",
        doctor.salary,
        care.weeks_kept_percent,
        medicine.care_cost_percent,
        medicine.after_effect_spared_percent
    )
}

/// What a recruiter costs and brings: "salaire 30 par semaine, 1 stat de plus
/// révélée par semaine, fourchettes à 70 %, potentiel estimé à 40 % près, personnalité
/// cachée".
pub fn recruiter_text(recruiter: &RecruiterRules) -> String {
    let scouting = &recruiter.scouting;
    let potential = match scouting.potential_range_percent {
        0 => "potentiel non estimé".to_owned(),
        percent => format!("potentiel estimé à {percent} % près"),
    };
    let personality = if scouting.personality {
        "personnalité vue"
    } else {
        "personnalité cachée"
    };
    format!(
        "salaire {} par semaine, {} stat de plus révélée par semaine, fourchettes à {} %, \
         {potential}, {personality}",
        recruiter.salary, scouting.revealed_per_week, scouting.range_width_percent
    )
}

/// What happened on the market, in one line, from the game after the week.
pub fn market_news_text(news: MarketNews, state: &GameState, data: &DesignData) -> String {
    let club = |i| club_fighter(i, state, data);
    let rival = |id| rival_fighter_name(id, state, data);
    match news {
        MarketNews::Signed(i) => format!("Recrute {}", club(i)),
        MarketNews::Released(i) => format!("Libère {}", club(i)),
        MarketNews::Sold(offer) => format!(
            "Vend {} à {} pour {}",
            club(offer.fighter),
            club_name(offer.rival, state),
            offer.price
        ),
        MarketNews::Bought {
            from,
            fighter,
            price,
        } => format!(
            "Achète {} à {} pour {price}",
            club(fighter),
            club_name(from.club, state)
        ),
        MarketNews::Countered(id, price) => format!(
            "{} demande {price} pour {} (offre possible cette semaine)",
            club_name(id.club, state),
            rival(id)
        ),
        MarketNews::ClubRefused(id) => format!(
            "{} refuse l'offre pour {}",
            club_name(id.club, state),
            rival(id)
        ),
        MarketNews::FighterRefused(id) => {
            format!("{} refuse de rejoindre le club", rival(id))
        }
        MarketNews::Failed(id) => format!(
            "Transfert de {} impossible : pas de place, pas assez d'argent ou combattant parti",
            rival(id)
        ),
    }
}

/// "Augure n° 3 (Ludus Magnus) à Capoue": who sent or would receive the challenge, and
/// where.
pub fn challenger_text(challenge: Challenge, state: &GameState, data: &DesignData) -> String {
    format!(
        "{} à {}",
        rival_fighter_name(challenge.rival, state, data),
        colosseum_name(challenge.colosseum, data)
    )
}

/// What happened to the title of the colosseum at index `c`, in one line.
pub fn title_news_text(c: usize, news: TitleNews, state: &GameState, data: &DesignData) -> String {
    let Some(rules) = data.world.colosseums.get(c) else {
        return String::new();
    };
    let name = |contender| contender_name(contender, state, data);
    let to = |to: Option<Contender>| to.map_or_else(|| "personne".into(), name);
    let place = &rules.name;
    match news {
        TitleNews::Fight {
            champion,
            challenger,
            result,
        } => {
            let outcome = if title_kept(result) {
                "le garde"
            } else {
                "le perd"
            };
            format!(
                "Titre de {place} : {} {outcome} face à {}",
                name(champion),
                name(challenger)
            )
        }
        TitleNews::Stripped { former, to: holder } => format!(
            "Titre de {place} : {} le perd faute de défense, il revient à {}",
            name(former),
            to(holder)
        ),
        TitleNews::Vacated { to: holder } => {
            format!("Titre de {place} vacant : il revient à {}", to(holder))
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
        } => format!(
            "{} achète {}, champion de {}, pour {price}",
            club_name(buyer, state),
            rival_fighter_name(fighter, state, data),
            data.world
                .colosseums
                .get(colosseum)
                .map_or("?", |c| c.name.as_str())
        ),
        WorldNews::Closed(club) => format!(
            "{} ferme, ruiné : ses combattants partent au marché",
            club_name(club, state)
        ),
        WorldNews::Rivalry(club, true) => format!(
            "{} se déclare votre rival : vos combats contre lui attirent les foules",
            club_name(club, state)
        ),
        WorldNews::Rivalry(club, false) => {
            format!("{} met fin à sa rivalité avec vous", club_name(club, state))
        }
        WorldNews::Founded(club) => state.rivals.get(club).map_or_else(String::new, |r| {
            format!(
                "Fondation de {} ({}), en {}",
                r.name,
                culture_name(&r.culture, data),
                continent_name(r.continent)
            )
        }),
    };
    format!("{} : {what}", date_text(news.week, data))
}

/// "Arrivé récemment : vendable à partir de la saison 1, semaine 16" for a fighter who
/// joined its club less than half a season ago, `None` for one that may be sold.
pub fn unsellable_text(
    fighter: &CareerFighter,
    state: &GameState,
    data: &DesignData,
) -> Option<String> {
    (!state.may_be_sold(fighter, data)).then(|| {
        let from = date_text(GameState::sellable_from(fighter, data), data);
        format!(
            "Arrivé récemment : vendable à partir de la {}",
            from.to_lowercase()
        )
    })
}

pub fn standing_name(standing: Standing) -> &'static str {
    match standing {
        Standing::Ally => "allié",
        Standing::Friend => "ami",
        Standing::Neutral => "neutre",
        Standing::Tense => "tendu",
        Standing::Rival => "rival",
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
        Reason::Killed { fighter, victim } => format!("{} a tué {}", club(fighter), rival(victim)),
        Reason::Beaten { fighter, rival: r } => format!("{} a battu {}", club(fighter), rival(r)),
        Reason::TitleTaken { fighter, colosseum } => format!(
            "{} lui a pris le titre de {}",
            club(fighter),
            data.world
                .colosseums
                .get(colosseum)
                .map_or("?", |c| c.name.as_str())
        ),
        Reason::ChallengeRefused { fighter, rival: r } => {
            format!("{} a refusé le défi de {}", club(fighter), rival(r))
        }
        Reason::Sold { fighter } => format!("vous lui avez vendu {}", club(fighter)),
        Reason::Bought { fighter } => format!("vous lui avez acheté {}", club(fighter)),
        Reason::PactKept { fighter, rival: r } => {
            format!(
                "{} a tenu son pacte avec {} en mêlée",
                club(fighter),
                rival(r)
            )
        }
        Reason::Betrayed { fighter, rival: r } => {
            format!(
                "{} a trahi son pacte avec {} en mêlée",
                club(fighter),
                rival(r)
            )
        }
    };
    format!(
        "{:+} : {why} ({})",
        change.points,
        date_text(change.week, data).to_lowercase()
    )
}

/// "Saison 2, semaine 7".
pub fn date_text(week: u64, data: &DesignData) -> String {
    let when = Date::of(week, &data.calendar);
    format!("Saison {}, semaine {}", when.season, when.week)
}

pub fn colosseum_name(id: ColosseumId, data: &DesignData) -> &str {
    data.world.colosseum(id).map_or("?", |c| c.name.as_str())
}

pub fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Cautious => "prudent",
        Profile::Aggressive => "agressif",
        Profile::Merchant => "marchand",
        Profile::Mentor => "formateur",
        Profile::Veteran => "vétéran",
        Profile::Showman => "spectacle",
    }
}

/// "pauvre", "modeste" or "riche".
pub fn wealth_name(wealth: Wealth) -> &'static str {
    match wealth {
        Wealth::Poor => "pauvre",
        Wealth::Modest => "modeste",
        Wealth::Rich => "riche",
    }
}

/// "Europe", "Asie", "Amériques", "Afrique" or "Océanie".
pub fn continent_name(continent: Continent) -> &'static str {
    match continent {
        Continent::Europe => "Europe",
        Continent::Asia => "Asie",
        Continent::Americas => "Amériques",
        Continent::Africa => "Afrique",
        Continent::Oceania => "Océanie",
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
    format!("Club {}", wealth_name(wealth))
}

/// The name of rival club `club`.
pub fn club_name(club: usize, state: &GameState) -> &str {
    state.rivals.get(club).map_or("?", |r| r.name.as_str())
}

/// The name of culture `key` ("Asie de l'Est").
pub fn culture_name<'a>(key: &'a str, data: &'a DesignData) -> &'a str {
    data.culture(key).map_or(key, |c| c.name.as_str())
}

/// "culture : Asie de l'Est".
pub fn culture_text(key: &str, data: &DesignData) -> String {
    format!("culture : {}", culture_name(key, data))
}

pub fn arena_name(arena: ArenaKind) -> &'static str {
    match arena {
        ArenaKind::Sand => "sable",
        ArenaKind::Pillars => "piliers",
        ArenaKind::Narrow => "étroite",
    }
}

/// What an arena kind does to the fight, for the player.
pub fn arena_effect(arena: ArenaKind) -> &'static str {
    match arena {
        ArenaKind::Sand => "le sable ralentit tout le monde, les tireurs en profitent",
        ArenaKind::Pillars => "les piliers arrêtent les tirs et barrent le passage",
        ArenaKind::Narrow => "le corps à corps s'engage vite",
    }
}

/// How a colosseum's ranking names `contender`; the club's fighters are marked.
pub fn contender_name(contender: Contender, state: &GameState, data: &DesignData) -> String {
    match contender {
        Contender::Club(i) => format!("{} (votre club)", club_fighter(i, state, data)),
        Contender::Rival(id) => rival_fighter_name(id, state, data),
        Contender::FreeAgent(a) => target_name(Target::FreeAgent(a), state, data),
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
        Severity::Lost if matches!(injury.location, Location::Leg(_)) => {
            return format!("{location} : perdue");
        }
        Severity::Lost => return format!("{location} : perdu"),
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

/// A severed limb in a sentence: "bras gauche tranché", "jambe droite tranchée".
fn limb_cut(location: Location) -> String {
    let side = |s, feminine| match (s, feminine) {
        (Side::Left, _) => "gauche",
        (Side::Right, false) => "droit",
        (Side::Right, true) => "droite",
    };
    match location {
        Location::Head => "tête tranchée".into(),
        Location::Body => "corps tranché".into(),
        Location::Arm(s) => format!("bras {} tranché", side(s, false)),
        Location::Leg(s) => format!("jambe {} tranchée", side(s, true)),
    }
}

pub fn fight_text(fight: &FightSummary, data: &DesignData) -> String {
    let mine = fight.mine();
    // Brought to 0 HP, it may still have survived the fatal blow.
    let fell = mine.is_some_and(|f| f.hp == 0);
    let knocked_out = mine.is_some_and(|f| f.knocked_out);
    let severed = mine.and_then(|f| f.severed);
    let result = match (fight.result, fight.died, severed) {
        (_, true, Some(Location::Head)) => "décapité".into(),
        (_, true, _) => "mort au combat".into(),
        (FightResult::Lost, false, Some(limb)) => format!("défaite, {}", limb_cut(limb)),
        (FightResult::Lost, false, _) if fell => {
            "laissé pour mort, survit grièvement blessé".into()
        }
        (FightResult::Lost, false, _) if knocked_out => "défaite, étourdi sans se relever".into(),
        (FightResult::Won, ..) => "victoire".into(),
        (FightResult::Lost, ..) => "défaite".into(),
        (FightResult::GaveUp, ..) => "abandon".into(),
        (FightResult::Unfinished, ..) => {
            let minutes = fight.start.max_ticks / (lanista_core::combat::TICKS_PER_SECOND * 60);
            match minutes {
                1 => "pas de vainqueur après une minute".into(),
                n => format!("pas de vainqueur après {n} minutes"),
            }
        }
    };
    let stronger = if fight.against_stronger {
        ", plus fort"
    } else {
        ""
    };
    let bare = match (fight.refused.armour, fight.refused.weapon) {
        (true, true) => ", sans armure ni arme",
        (true, false) => ", sans armure",
        (false, true) => ", sans arme",
        (false, false) => "",
    };
    let against = if fight.is_melee() {
        format!(
            "Mêlée à {}, {}{}",
            fight.opponents.len() + 1,
            place_name(fight.place),
            if fight.against_stronger {
                ", plus forts"
            } else {
                ""
            }
        )
    } else {
        let class = fight
            .opponent()
            .map_or_else(|| "?".into(), |o| class_name(o, data));
        format!("Contre un {class}{stronger}")
    };
    format!(
        "{against}{bare} : {result}, renommée {:+}",
        fight.fame_change
    )
}

/// A place in a melee: « 1er », « 2e »…
pub fn place_name(place: u32) -> String {
    if place == 1 {
        "1er".into()
    } else {
        format!("{place}e")
    }
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
                Flow::ChampionBonus => "bonus champion",
                Flow::Purchase => "achat",
                Flow::Sale => "vente",
                Flow::Salary => "salaires",
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
    if report.after_effect_spared {
        lines.push("Séquelle évitée par le médecin".into());
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
        if report.season_gains > 0 {
            lines.push(format!(
                "Bilan de la saison : {} progrès",
                report.season_gains
            ));
        }
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
        Status::Active | Status::Dead | Status::Sold | Status::Released => {}
        Status::Retired if too_maimed(&after.injuries, &data.fighter.retirement) => {
            lines.push("Prend sa retraite : trop mutilé pour combattre".into());
        }
        Status::Retired => lines.push(format!("Prend sa retraite à {} ans", after.age)),
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
        let text = fight_text(&summary, &data);
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
